use std::{
    fs::{self, File},
    io::{self, BufWriter, Seek, SeekFrom, Write},
    path::Path,
    pin::Pin,
};

use memmap2::MmapMut;
use rkyv::bytecheck;
use rkyv::{
    ser::{serializers::WriteSerializer, Serializer},
    validation::validators::DefaultValidator,
    Archive, CheckBytes, Serialize,
};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("ref outside of range")]
    RefOutsideRange,
    #[error("validation error: {0}")]
    Validate(String),
}

pub type Result<T> = std::result::Result<T, Error>;
pub type FileArraySerializer<'a> = WriteSerializer<BufWriter<&'a mut File>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Archive)]
#[archive_attr(derive(CheckBytes))]
pub struct Ref(usize);

impl Ref {
    pub fn as_usize(self) -> usize {
        self.0
    }

    fn as_u64(self) -> u64 {
        self.0.try_into().expect("expecting 64 bit arch")
    }

    pub fn null() -> Self {
        Self(0)
    }

    pub fn is_null(self) -> bool {
        self.0 == 0
    }
}

impl From<usize> for Ref {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl From<u64> for Ref {
    fn from(value: u64) -> Self {
        Self(value.try_into().expect("expecting 64 bit arch"))
    }
}

impl From<Ref> for usize {
    fn from(value: Ref) -> Self {
        value.as_usize()
    }
}

impl From<Ref> for u64 {
    fn from(value: Ref) -> Self {
        value.as_u64()
    }
}

impl From<&ArchivedRef> for Ref {
    fn from(value: &ArchivedRef) -> Self {
        value.0.into()
    }
}

impl ArchivedRef {
    fn as_mut_u64(self: Pin<&mut Self>) -> &mut u64 {
        unsafe { &mut self.get_unchecked_mut().0 }
    }

    pub fn set(self: Pin<&mut Self>, new_ref: Ref) {
        *self.as_mut_u64() = new_ref.as_u64();
    }

    pub fn to_ref(&self) -> Ref {
        self.into()
    }
}

type HEADER = usize;
const HEADER_SIZE: usize = std::mem::size_of::<HEADER>();

pub struct FileArray {
    mmap: MmapMut,
    file: File,
}

/// A file backed memory area. New values can be appended, but not removed. Zero-copy
/// deserialization using rkyv. Is not platform-independent since the stored values need
/// to be aligned for the current platform, and `usize` is different sizes.
impl FileArray {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        // TODO: flock using fs2?
        let file = fs::OpenOptions::new().read(true).create(true).open(path)?;
        Self::new_opened(file)
    }

    fn new_opened(mut file: File) -> Result<Self> {
        let file_len = file.seek(SeekFrom::End(0))?;
        if file_len == 0 {
            WriteSerializer::new(&mut file).serialize_value(&HEADER_SIZE)?;
        }

        // TODO: how to handle the signal that gets sent when the mapped file becomes
        // unavailable? Just return errors instead of crashing at least. SIGBUS
        let mmap = unsafe { MmapMut::map_mut(&file)? };
        mmap.advise(memmap2::Advice::Random)?;
        mmap.advise(memmap2::Advice::DontFork)?;

        let len = mmap.len();
        assert!(len >= HEADER_SIZE);

        let mut fa = Self { file, mmap };
        fa.file.seek(SeekFrom::Start(
            fa.len().try_into().expect("expecting 64 bit arch"),
        ))?;
        Ok(fa)
    }

    pub fn is_empty(&self) -> bool {
        self.len() <= HEADER_SIZE
    }

    pub fn copy_to<W>(&mut self, mut writer: W) -> Result<()>
    where
        W: Write,
    {
        let res = || -> Result<()> {
            self.file.seek(SeekFrom::Start(0))?;
            let mut buf = BufReader::new(&self.file);
            std::io::copy(&mut buf, &mut writer)?;
            Ok(())
        }();

        self.file.seek(SeekFrom::End(0))?;
        res
    }

    pub fn len(&self) -> usize {
        // TODO: just use a pointer?
        self
            // TODO: use unsafe variants without checkbytes
            .get::<HEADER>(HEADER_SIZE.into())
            .expect("should always exist")
            .to_owned()
            .try_into()
            .expect("expecting 64 bit arch")
    }

    fn set_len(&mut self, new_len: usize) {
        *self
            // TODO: use unsafe variants without checkbytes
            .get_mut::<HEADER>(HEADER_SIZE.into())
            .expect("should always exist") =
            new_len.try_into().expect("expecting 64 bit");
    }

    pub fn ref_to_first<T>() -> Ref {
        let pos = HEADER_SIZE;
        let align = std::mem::align_of::<T>();
        dbg!(align);
        let align_diff = (align - (pos % align)) % align;
        (pos + align_diff + std::mem::size_of::<T>()).into()
    }

    pub fn reserve(&mut self, additional: usize) -> Result<()> {
        let new_len = self.len() + additional;
        self.file
            .set_len(new_len.try_into().expect("expecting 64 bit arch"))?;

        unsafe {
            // TODO: are the advices preserved?
            self.mmap
                .remap(new_len, memmap2::RemapOptions::new().may_move(true))?;
        }

        Ok(())
    }

    pub fn add<'i, It, S>(&mut self, items: It) -> Result<Vec<Ref>>
    where
        It: IntoIterator<Item = &'i S>,
        S: for<'s> Serialize<FileArraySerializer<'s>> + 'i,
    {
        let mut ser = {
            let len = self.len();
            let buf = BufWriter::new(&mut self.file);
            WriteSerializer::with_pos(buf, len)
        };

        let mut refs: Vec<Ref> = Vec::new();

        for item in items.into_iter() {
            ser.align_for::<S>()?;
            ser.serialize_value(item)?;
            refs.push(ser.pos().into());
        }

        ser.into_inner().flush()?;
        if let Some(&last_ref) = refs.last() {
            self.set_len(last_ref.into());
        }

        if self.len() > self.mmap.len() {
            const GROWTH: usize = 1 << 13;
            self.reserve(GROWTH)?;
        }

        Ok(refs)
    }

    pub fn add_one<S>(&mut self, item: &S) -> Result<Ref>
    where
        S: for<'a> Serialize<FileArraySerializer<'a>>,
    {
        self.add([item])
            .map(|vec| vec.into_iter().next().expect("should have exactly one"))
    }

    // TODO: have unsafe getters as an alternative?
    pub fn get<'a, D>(&'a self, key: Ref) -> Result<&'a D::Archived>
    where
        D: Archive,
        D::Archived: CheckBytes<DefaultValidator<'a>>,
    {
        let slice = self
            .mmap
            .get(..key.as_usize())
            .ok_or(Error::RefOutsideRange)?;
        Ok(rkyv::check_archived_root::<D>(slice)
            .map_err(|e| Error::Validate(format!("{e}")))?)
    }

    pub fn get_mut<'a, D>(&'a mut self, key: Ref) -> Result<Pin<&'a mut D::Archived>>
    where
        D: Archive,
        D::Archived: for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        let slice = self
            .mmap
            .get_mut(..key.as_usize())
            .ok_or(Error::RefOutsideRange)?;
        // TODO: https://github.com/rkyv/rkyv/issues/260
        rkyv::check_archived_root::<D>(slice)
            .map_err(|e| Error::Validate(format!("{e}")))?;
        Ok(unsafe { rkyv::archived_root_mut::<D>(Pin::new(slice)) })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rkyv::bytecheck;
    use tempfile::tempfile;

    #[derive(Archive, Serialize)]
    #[archive_attr(derive(CheckBytes))]
    struct MyStuff {
        a: i32,
        b: String,
    }

    // TODO: how to use pin-project?
    impl ArchivedMyStuff {
        fn a(self: Pin<&mut Self>) -> &mut i32 {
            unsafe { &mut self.get_unchecked_mut().a }
        }
    }

    #[test]
    fn add_empty() -> Result<()> {
        let tmpf = tempfile()?;
        let mut arr = FileArray::new_opened(tmpf)?;
        let len_before = arr.len();
        let mmap_len_before = arr.mmap.len();
        arr.add([] as [&i32; 0])?;
        assert_eq!(len_before, arr.len());
        assert_eq!(mmap_len_before, arr.mmap.len());
        Ok(())
    }

    #[test]
    fn basic_add() -> Result<()> {
        let tmpf = tempfile()?;
        let mut arr = FileArray::new_opened(tmpf)?;

        let mmap_len_before = arr.mmap.len();
        assert_eq!(HEADER_SIZE, arr.len());

        assert!(matches!(
            arr.get::<i32>(1000u64.into()),
            Err(Error::RefOutsideRange)
        ));
        assert!(matches!(
            arr.get::<i32>(0u64.into()),
            Err(Error::Validate(_))
        ));
        assert!(matches!(arr.get::<()>(0u64.into()), Ok(_)));

        let first_ref = arr.add_one(&123i32)?;
        assert!(arr.len() > HEADER_SIZE);
        assert!(arr.mmap.len() > mmap_len_before);

        let first = arr.get::<i32>(first_ref)?;
        assert_eq!(&123, first);
        assert_eq!(first_ref, FileArray::ref_to_first::<i32>());
        assert_eq!(first_ref.as_usize(), arr.len());

        Ok(())
    }

    #[test]
    fn mutate() -> Result<()> {
        let tmpf = tempfile()?;
        let tmpf2 = tmpf.try_clone()?;
        let mut arr = FileArray::new_opened(tmpf)?;

        let ele_ref = arr.add_one(&MyStuff {
            a: 0,
            b: "hejsan".to_string(),
        })?;

        let mut my_stuff = arr.get_mut::<MyStuff>(ele_ref)?;
        assert_eq!(0, my_stuff.a);
        *my_stuff.as_mut().a() = 1;
        assert_eq!(1, my_stuff.a);

        drop(arr);
        let arr = FileArray::new_opened(tmpf2)?;
        let my_stuff = arr.get::<MyStuff>(ele_ref)?;
        assert_eq!(1, my_stuff.a);
        assert_eq!(ele_ref.as_usize(), arr.len());

        Ok(())
    }

    #[test]
    fn add_many() -> Result<()> {
        let tmpf = tempfile()?;
        let mut arr = FileArray::new_opened(tmpf)?;

        let refs = arr.add([&1i32, &10, &100])?;
        assert_eq!(&1, arr.get::<i32>(refs[0])?);
        assert_eq!(&10, arr.get::<i32>(refs[1])?);
        assert_eq!(&100, arr.get::<i32>(refs[2])?);
        assert_eq!(refs.last().unwrap().as_usize(), arr.len());

        Ok(())
    }

    #[test]
    fn reopen() -> Result<()> {
        let tmpf = tempfile()?;
        let mut tmpf2 = tmpf.try_clone()?;
        let mut tmpf3 = tmpf.try_clone()?;

        let mut arr = FileArray::new_opened(tmpf)?;
        let ref_1 = arr.add_one(&1u32)?;
        drop(arr);

        tmpf2.seek(SeekFrom::Start(0))?;
        let mut arr = FileArray::new_opened(tmpf2)?;
        let ref_2 = arr.add_one(&2i64)?;
        drop(arr);

        tmpf3.seek(SeekFrom::Start(0))?;
        let arr = FileArray::new_opened(tmpf3)?;
        assert_eq!(arr.len(), ref_2.as_usize());
        assert!(arr.len() <= arr.mmap.len());
        assert_eq!(&1u32, arr.get::<u32>(ref_1)?);
        assert_eq!(&2i64, arr.get::<i64>(ref_2)?);
        assert_eq!(ref_1, FileArray::ref_to_first::<u32>());

        Ok(())
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn alignment_x86_64() {
        assert_eq!(Ref(16), FileArray::ref_to_first::<u64>());
        assert_eq!(Ref(16), FileArray::ref_to_first::<usize>());
        assert_eq!(Ref(9), FileArray::ref_to_first::<u8>());
        assert_eq!(Ref(24), FileArray::ref_to_first::<u128>());
        assert_eq!(Ref(40), FileArray::ref_to_first::<MyStuff>());
    }

    #[test]
    fn copy_to_writer() -> Result<()> {
        let tmpf = tempfile()?;
        let mut arr = FileArray::new_opened(tmpf)?;
        arr.add_one(&123u8)?;

        let mut buf = Vec::new();
        arr.copy_to(&mut buf)?;

        assert!(buf.len() >= HEADER_SIZE + std::mem::size_of::<u8>());

        let pos = FileArray::ref_to_first::<u8>().as_usize();
        assert_eq!(123u8, buf[pos - 1]);

        Ok(())
    }
}
