use std::{
    fs::{self, File},
    io::{self, BufReader, BufWriter, Seek, SeekFrom, Write},
    path::Path,
    pin::Pin,
};

use derivative::Derivative;
use memmap2::MmapMut;
use rkyv::{
    bytecheck,
    ser::serializers::{
        AllocScratch, AllocScratchError, CompositeSerializer, CompositeSerializerError,
        FallbackScratch, HeapScratch,
    },
};
use rkyv::{
    ser::{serializers::WriteSerializer, Serializer},
    validation::validators::DefaultValidator,
    Archive, CheckBytes, Serialize,
};

// TODO: add backtraces when it is stable
// https://github.com/dtolnay/thiserror/issues/204
// https://github.com/dtolnay/thiserror/issues/236
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("serializer: {0}")]
    Serializer(
        #[from]
        // TODO: why doesn't `<FileArraySerializer as Fallible>::Error` work?
        CompositeSerializerError<io::Error, AllocScratchError, std::convert::Infallible>,
    ),
    #[error("ref outside of range")]
    RefOutsideRange,
    #[error("validation error: {0}")]
    Validate(String),
    #[error("NullPointerException")]
    NullRef,
}

pub type Result<T> = std::result::Result<T, Error>;
pub type FileArraySerializer = CompositeSerializer<
    WriteSerializer<BufWriter<File>>,
    FallbackScratch<HeapScratch<8192>, AllocScratch>,
>;

#[derive(Derivative, CheckBytes)]
#[derivative(
    Debug(bound = ""),
    Copy(bound = ""),
    Clone(bound = ""),
    PartialEq(bound = ""),
    Eq(bound = "")
)]
#[repr(transparent)]
pub struct Ref<T> {
    offset: u64,
    _t: std::marker::PhantomData<T>,
}

// The automatic impl also requires that `T` is Unpin, but that doesn't matter in this
// case.
impl<T> Unpin for Ref<T> {}

impl<T> rkyv::Archive for Ref<T> {
    type Archived = Self;
    type Resolver = ();

    unsafe fn resolve(
        &self,
        _pos: usize,
        _resolver: Self::Resolver,
        out: *mut Self::Archived,
    ) {
        out.write(*self);
    }
}

impl<S: rkyv::Fallible + ?Sized, T> rkyv::Serialize<S> for Ref<T> {
    fn serialize(
        &self,
        _serializer: &mut S,
    ) -> std::result::Result<Self::Resolver, <S as rkyv::Fallible>::Error> {
        Ok(())
    }
}

impl<T> Ref<T> {
    pub fn as_usize(self) -> usize {
        self.offset.try_into().expect("expecting 64 bit arch")
    }

    pub const fn as_u64(self) -> u64 {
        self.offset
    }

    pub const fn null() -> Self {
        Self::new_u64(0)
    }

    pub const fn is_null(self) -> bool {
        self.offset == 0
    }

    pub const fn is_not_null(self) -> bool {
        !self.is_null()
    }

    const fn new_u64(offset: u64) -> Self {
        Self {
            offset,
            _t: std::marker::PhantomData,
        }
    }

    fn new_usize(offset: usize) -> Self {
        Self::new_u64(offset.try_into().expect("expecting 64 bit arch"))
    }
}

impl<T> From<Ref<T>> for usize {
    fn from(value: Ref<T>) -> Self {
        value.as_usize()
    }
}

impl<T> From<Ref<T>> for u64 {
    fn from(value: Ref<T>) -> Self {
        value.as_u64()
    }
}

type HEADER = usize;
const HEADER_SIZE: usize = std::mem::size_of::<HEADER>();

pub struct FileArray {
    mmap: MmapMut,
    seri: FileArraySerializer,
}

/// A file backed memory area. New values can be appended, but not removed. Zero-copy
/// deserialization using rkyv. Is not platform-independent since the stored values need
/// to be aligned for the current platform, endianess, and `usize` is different sizes.
impl FileArray {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        // TODO: flock using fs2?
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;
        Self::new_opened(file)
    }

    fn new_opened(mut file: File) -> Result<Self> {
        // TODO: double check open options on the file. Read, write and not append
        let file_len = file.seek(SeekFrom::End(0))?;
        if file_len == 0 {
            WriteSerializer::new(&mut file).serialize_value(&HEADER_SIZE)?;
        }

        // TODO: how to handle the signal that gets sent when the mapped file becomes
        // unavailable? Just return errors instead of crashing at least. SIGBUS
        let mmap = unsafe { MmapMut::map_mut(&file)? };
        mmap.advise(memmap2::Advice::Random)?;
        mmap.advise(memmap2::Advice::DontFork)?;

        let total_len = mmap.len();
        assert!(total_len >= HEADER_SIZE);

        let used_len = Self::len_raw(&mmap);
        file.seek(SeekFrom::Start(
            used_len.try_into().expect("expecting 64 bit arch"),
        ))?;
        let seri = CompositeSerializer::new(
            WriteSerializer::with_pos(BufWriter::new(file), used_len),
            FallbackScratch::default(),
            rkyv::Infallible,
        );

        Ok(Self { mmap, seri })
    }

    #[cfg(test)]
    pub fn new_tempfile() -> Result<Self> {
        // TODO: maybe use https://docs.rs/memfd/latest/memfd/ instead?
        let tmpf = tempfile::tempfile()?;
        Self::new_opened(tmpf)
    }

    #[cfg(test)]
    pub fn clone_filehandle(&mut self) -> io::Result<File> {
        self.with_file(|file| file.get_ref().try_clone())
    }

    #[cfg(test)]
    pub fn raw_data(&self) -> &[u8] {
        let len = self.len();
        &self.mmap[..len]
    }

    pub fn is_empty(&self) -> bool {
        self.len() <= HEADER_SIZE
    }

    pub fn sync_to_disk(&self) -> Result<()> {
        // TODO: fsync on the file instead? Is there any difference?
        Ok(self.mmap.flush()?)
    }

    pub fn copy_to<W>(&mut self, mut writer: W) -> Result<()>
    where
        W: Write,
    {
        self.with_file(|file| -> Result<()> {
            let original_pos = file.seek(SeekFrom::Current(0))?;

            let res = || -> Result<()> {
                file.seek(SeekFrom::Start(0))?;
                let mut buf = BufReader::new(file.get_mut());
                std::io::copy(&mut buf, &mut writer)?;
                Ok(())
            }();

            file.seek(SeekFrom::Start(original_pos))?;
            res
        })
    }

    pub fn len(&self) -> usize {
        Self::len_raw(&self.mmap)
    }

    pub fn len_raw(slice: &[u8]) -> usize {
        // TODO: just use a pointer?
        // TODO: use unsafe variants without checkbytes
        Self::get_raw::<HEADER>(slice, Ref::new_usize(HEADER_SIZE))
            .expect("should always exist")
            .to_owned()
            .try_into()
            .expect("expecting 64 bit arch")
    }

    fn set_len(&mut self, new_len: usize) {
        *self
            // TODO: use unsafe variants without checkbytes
            .get_mut::<HEADER>(Ref::new_usize(HEADER_SIZE))
            .expect("should always exist") =
            new_len.try_into().expect("expecting 64 bit");
    }

    /// Ref to the first element of type `T`, whose serialized size must be
    /// `size_of<T::Archived>`, i.e., should not have stuff like strings cuz they get
    /// serialized before `T`, making it hard to get the position of `T`.
    pub fn ref_to_first<T>() -> Ref<T>
    where
        T: Archive,
    {
        let pos = HEADER_SIZE;
        let align = std::mem::align_of::<T::Archived>();
        let align_diff = (align - (pos % align)) % align;
        Ref::new_usize(pos + align_diff + std::mem::size_of::<T::Archived>())
    }

    fn with_file<F, R>(&mut self, appl: F) -> R
    where
        F: FnOnce(&mut BufWriter<File>) -> R,
    {
        replace_with::replace_with_and_return(
            &mut self.seri,
            || {
                // NOTE: just to replace it with anything to allow the panic to keep
                // propagating
                CompositeSerializer::new(
                    WriteSerializer::new(BufWriter::new(
                        File::open("/dev/null").expect("should exist"),
                    )),
                    FallbackScratch::default(),
                    rkyv::Infallible,
                )
            },
            |seri| {
                let (write_seri, c, h) = seri.into_components();
                let pos = write_seri.pos();
                let mut bufwriter = write_seri.into_inner();

                let res = appl(&mut bufwriter);

                let write_seri = WriteSerializer::with_pos(bufwriter, pos);
                let seri = CompositeSerializer::new(write_seri, c, h);

                (res, seri)
            },
        )
    }

    pub fn reserve(&mut self, additional: usize) -> Result<()> {
        self.reserve_internal(additional, self.mmap.len())
    }

    fn reserve_internal(&mut self, additional: usize, file_len: usize) -> Result<()> {
        let new_len = file_len + additional;
        let new_len_u64: u64 = new_len.try_into().expect("expecting 64 bit arch");

        self.with_file(|file| file.get_mut().set_len(new_len_u64))?;
        unsafe {
            // TODO: are the advices preserved?
            self.mmap
                .remap(new_len, memmap2::RemapOptions::new().may_move(true))?;
        }

        Ok(())
    }

    pub fn add<'i, It, S>(&mut self, items: It) -> Result<Vec<Ref<S>>>
    where
        It: IntoIterator<Item = &'i S>,
        S: Serialize<FileArraySerializer> + 'i,
    {
        let mut refs: Vec<Ref<S>> = Vec::new();

        for item in items.into_iter() {
            // TODO: make sure sync_to_disk always is called if this fails?
            self.seri.serialize_value(item)?;
            refs.push(Ref::new_usize(self.seri.pos()));
        }

        self.with_file(|file| file.flush())?;

        if let Some(&last_ref) = refs.last() {
            self.set_len(last_ref.into());
        }

        if self.len() > self.mmap.len() {
            const GROWTH: usize = 1 << 13;
            self.reserve_internal(GROWTH, self.len())?;
        }

        Ok(refs)
    }

    pub fn add_one<S>(&mut self, item: &S) -> Result<Ref<S>>
    where
        S: Serialize<FileArraySerializer>,
    {
        // TODO: write directly into the mmap using `BufferSerializer` or something?
        self.add([item])
            .map(|vec| vec.into_iter().next().expect("should have exactly one"))
    }

    // TODO: have unsafe getters as an alternative?
    pub fn get<'a, D>(&'a self, key: Ref<D>) -> Result<&'a D::Archived>
    where
        D: Archive,
        D::Archived: CheckBytes<DefaultValidator<'a>>,
    {
        Self::get_raw::<D>(&self.mmap, key)
    }

    fn get_raw<'a, D>(slice: &'a [u8], key: Ref<D>) -> Result<&'a D::Archived>
    where
        D: Archive,
        D::Archived: CheckBytes<DefaultValidator<'a>>,
    {
        if key.is_null() {
            return Err(Error::NullRef);
        }
        let slice = slice.get(..key.as_usize()).ok_or(Error::RefOutsideRange)?;
        Ok(rkyv::check_archived_root::<D>(slice)
            .map_err(|e| Error::Validate(format!("{e}")))?)
    }

    pub fn get_mut<'a, D>(&'a mut self, key: Ref<D>) -> Result<Pin<&'a mut D::Archived>>
    where
        D: Archive,
        D::Archived: for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        if key.is_null() {
            return Err(Error::NullRef);
        }
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
        let mut arr = FileArray::new_tempfile()?;
        let len_before = arr.len();
        let mmap_len_before = arr.mmap.len();
        arr.add([] as [&i32; 0])?;
        assert_eq!(len_before, arr.len());
        assert_eq!(mmap_len_before, arr.mmap.len());
        Ok(())
    }

    #[test]
    fn basic_add() -> Result<()> {
        let mut arr = FileArray::new_tempfile()?;

        let mmap_len_before = arr.mmap.len();
        assert_eq!(HEADER_SIZE, arr.len());

        assert!(matches!(
            arr.get::<i32>(Ref::new_u64(1000)),
            Err(Error::RefOutsideRange)
        ));
        assert!(matches!(
            arr.get::<i32>(Ref::new_u64(0)),
            Err(Error::NullRef)
        ));
        assert!(matches!(
            arr.get::<()>(Ref::new_u64(0)),
            Err(Error::NullRef)
        ));

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
        let mut arr = FileArray::new_tempfile()?;
        let tmpf2 = arr.clone_filehandle()?;

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
        let mut arr = FileArray::new_tempfile()?;

        let refs = arr.add([&1i32, &10, &100])?;
        assert_eq!(&1, arr.get::<i32>(refs[0])?);
        assert_eq!(&10, arr.get::<i32>(refs[1])?);
        assert_eq!(&100, arr.get::<i32>(refs[2])?);
        assert_eq!(refs.last().unwrap().as_usize(), arr.len());

        Ok(())
    }

    #[test]
    fn reopen() -> Result<()> {
        let mut arr = FileArray::new_tempfile()?;
        let mut tmpf2 = arr.clone_filehandle()?;
        let mut tmpf3 = arr.clone_filehandle()?;

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
        assert_eq!(Ref::new_u64(16), FileArray::ref_to_first::<u64>());
        assert_eq!(Ref::new_u64(16), FileArray::ref_to_first::<usize>());
        assert_eq!(Ref::new_u64(9), FileArray::ref_to_first::<u8>());
        assert_eq!(Ref::new_u64(24), FileArray::ref_to_first::<u128>());
        assert_eq!(Ref::new_u64(32), FileArray::ref_to_first::<MyStuff>());
    }

    #[test]
    fn copy_to_writer() -> Result<()> {
        let mut arr = FileArray::new_tempfile()?;
        arr.add_one(&123u8)?;

        let mut buf = Vec::new();
        arr.copy_to(&mut buf)?;

        assert!(buf.len() >= HEADER_SIZE + std::mem::size_of::<u8>());

        let pos = FileArray::ref_to_first::<u8>().as_usize();
        assert_eq!(123u8, buf[pos - 1]);

        Ok(())
    }
}