use std::{
    fs::{self, File},
    io::{self, BufWriter, Seek, SeekFrom, Write},
    path::Path,
    pin::Pin,
};

use memmap2::MmapMut;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ref(pub u64);

impl Ref {
    pub fn as_u64(self) -> u64 {
        self.0
    }

    pub fn as_usize(self) -> usize {
        self.0.try_into().expect("should never fail on 64 bit")
    }
}

impl From<u64> for Ref {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<usize> for Ref {
    fn from(value: usize) -> Self {
        Self(
            value
                .try_into()
                .expect("shouldn't fail unless on a >64 bit system"),
        )
    }
}

impl From<Ref> for u64 {
    fn from(value: Ref) -> Self {
        value.as_u64()
    }
}

impl From<Ref> for usize {
    fn from(value: Ref) -> Self {
        value.as_usize()
    }
}

pub struct FileArray {
    mmap: MmapMut,
    file: File,
    len: usize,
}

impl FileArray {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let file = fs::OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(path)?;

        Self::new_opened(file)
    }

    fn new_opened(mut file: File) -> Result<Self> {
        // TODO: how to handle the signal that gets sent when the mapped file becomes
        // unavailable? Just return errors instead of crashing at least. SIGBUS
        let mmap = unsafe { MmapMut::map_mut(&file)? };
        mmap.advise(memmap2::Advice::Random)?;
        mmap.advise(memmap2::Advice::DontFork)?;

        let len = mmap.len();

        file.seek(SeekFrom::End(0))?;

        Ok(Self { file, mmap, len })
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn ref_to_first<T>() -> Ref {
        std::mem::size_of::<T>().into()
    }

    pub fn add<'s, 'i, It, S>(&'s mut self, items: It) -> Result<Vec<Ref>>
    where
        It: IntoIterator<Item = &'i S>,
        S: Serialize<WriteSerializer<BufWriter<&'s mut File>>> + 'i,
    {
        let buf = BufWriter::new(&mut self.file);
        let mut ser = WriteSerializer::with_pos(buf, self.len);

        let mut refs: Vec<Ref> = Vec::new();

        for item in items.into_iter() {
            ser.align_for::<S>()?;
            ser.serialize_value(item)?;
            refs.push(ser.pos().into());
        }

        ser.into_inner().flush()?;
        self.len = refs.last().map(|x| (*x).into()).unwrap_or(self.len);

        unsafe {
            // TODO: are the advices preserved?
            self.mmap
                .remap(self.len, memmap2::RemapOptions::new().may_move(true))?;
        }

        Ok(refs)
    }

    pub fn add_one<'a, S>(&'a mut self, item: &S) -> Result<Ref>
    where
        S: Serialize<WriteSerializer<BufWriter<&'a mut File>>>,
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
    fn basic_add() -> Result<()> {
        let tmpf = tempfile()?;
        let mut arr = FileArray::new_opened(tmpf)?;

        assert_eq!(0, arr.len);
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
        assert!(arr.len > 0);
        let first = arr.get::<i32>(first_ref)?;
        assert_eq!(&123, first);
        assert_eq!(first_ref, FileArray::ref_to_first::<i32>());

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

        Ok(())
    }
}
