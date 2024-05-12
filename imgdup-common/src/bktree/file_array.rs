use std::{
    borrow::Borrow,
    fs::{self, File},
    io::{self, BufWriter, Seek, SeekFrom, Write},
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
        FileArraySerializerError,
    ),
    #[error("ref outside of range")]
    RefOutsideRange,
    #[error("validation error: {0}")]
    Validate(String),
    #[error("NullPointerException")]
    NullRef,
}

pub type Result<T> = std::result::Result<T, Error>;

pub type FileArraySerializerError =
    CompositeSerializerError<io::Error, AllocScratchError, std::convert::Infallible>;

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

// TODO: somehow save the expected architecture too
type HEADER = usize;
const HEADER_SIZE: usize = std::mem::size_of::<HEADER>();

/// A file backed memory area. New values can be appended, but not removed. Zero-copy
/// deserialization using rkyv. Is not platform-independent since the stored values need
/// to be aligned for the current platform, endianess, and `usize` is different sizes.
///
/// Invariants:
///   - The buffer in the `BufWriter` must be empty
///   - The mmap always maps the whole file, padding and all
///   - The used length is less than or equal to the file length
pub struct FileArray {
    mmap: MmapMut,
    seri: FileArraySerializer,
}

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

        let total_len = mmap.len();
        assert!(total_len >= HEADER_SIZE);

        let used_len = Self::len_raw(&mmap);
        file.seek(SeekFrom::Start(
            used_len.try_into().expect("expecting 64 bit arch"),
        ))?;
        let seri = Self::new_serializer(file, used_len);

        Ok(Self { mmap, seri })
    }

    fn new_serializer(file: File, used_len: usize) -> FileArraySerializer {
        CompositeSerializer::new(
            WriteSerializer::with_pos(BufWriter::new(file), used_len),
            FallbackScratch::default(),
            rkyv::Infallible,
        )
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

    #[allow(dead_code)]
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

    pub fn len(&self) -> usize {
        Self::len_raw(&self.mmap)
    }

    fn len_raw(slice: &[u8]) -> usize {
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

    // TODO: make a unit test for this
    fn reset_serializer(&mut self) {
        let used_len = self.len();
        replace_with::replace_with_or_abort(&mut self.seri, |seri| {
            // NOTE: none of these should be able to fail, so its safe to use the abort
            // version of replace_with
            let (write_seri, _, _) = seri.into_components();
            let bufwriter = write_seri.into_inner();
            let (file, _) = bufwriter.into_parts();
            Self::new_serializer(file, used_len)
        });
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

    pub fn truncate(&mut self) -> Result<()> {
        if self.len() != self.mmap.len() {
            self.reserve_internal(0, self.len())?;
        }
        Ok(())
    }

    #[allow(dead_code)] // NOTE: will maybe be used one day
    pub fn reserve(&mut self, additional: usize) -> Result<()> {
        self.reserve_internal(additional, self.mmap.len())
    }

    fn reserve_internal(&mut self, additional: usize, file_len: usize) -> Result<()> {
        let new_len = file_len + additional;
        let new_len_u64: u64 = new_len.try_into().expect("expecting 64 bit arch");

        self.with_file(|file| file.get_mut().set_len(new_len_u64))?;
        unsafe {
            self.mmap
                .remap(new_len, memmap2::RemapOptions::new().may_move(true))?;
        }

        Ok(())
    }

    pub fn add<It, B, S>(&mut self, items: It) -> Result<Vec<Ref<S>>>
    where
        It: IntoIterator<Item = B>,
        B: Borrow<S>,
        S: Serialize<FileArraySerializer>,
    {
        let refs_res = || -> Result<_> {
            let mut refs: Vec<Ref<S>> = Vec::new();

            for item in items.into_iter() {
                self.seri.serialize_value(item.borrow())?;
                refs.push(Ref::new_usize(self.seri.pos()));
            }

            // TODO: how to make a unit test when flush fails?
            self.with_file(|buf| buf.flush())?;

            Ok(refs)
        }();

        let refs = match refs_res {
            Err(e) => {
                self.reset_serializer();
                return Err(e.into());
            }
            Ok(refs) => refs,
        };

        if let Some(&last_ref) = refs.last() {
            self.set_len(last_ref.into());
        }

        if self.len() > self.mmap.len() {
            let growth: usize = std::cmp::max(1 << 13 /*8K*/, self.len());
            self.reserve_internal(growth, self.len())?;
        }

        Ok(refs)
    }

    pub fn add_one<B, S>(&mut self, item: B) -> Result<Ref<S>>
    where
        B: Borrow<S>,
        S: Serialize<FileArraySerializer>,
    {
        self.add([item])
            .map(|vec| vec.into_iter().next().expect("should have exactly one"))
    }

    // TODO: have unsafe getters that don't check the bytes as an alternative?
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
        arr.add::<[&i32; 0], &i32, i32>([] as [&i32; 0])?;
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

        let refs = arr.add([2i32, 20, 200])?;
        assert_eq!(&2, arr.get::<i32>(refs[0])?);
        assert_eq!(&20, arr.get::<i32>(refs[1])?);
        assert_eq!(&200, arr.get::<i32>(refs[2])?);
        assert_eq!(refs.last().unwrap().as_usize(), arr.len());

        // NOTE: there are several ways this `Box` can be borrowed, so the compiler
        // requires an explicit type on `S`, which is good.
        let refs: Vec<Ref<i32>> = arr.add([Box::new(5i32)])?;
        assert_eq!(&5, arr.get::<i32>(refs[0])?);
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
        assert_eq!(Ref::new_u64(32), FileArray::ref_to_first::<u128>());
        assert_eq!(Ref::new_u64(32), FileArray::ref_to_first::<MyStuff>());
    }
}
