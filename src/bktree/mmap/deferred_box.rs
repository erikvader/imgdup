use std::{borrow::Borrow, pin::Pin};

use rkyv::{
    ser::{
        serializers::{
            AlignedSerializer, AllocScratch, AllocScratchError, CompositeSerializer,
            CompositeSerializerError, FallbackScratch, HeapScratch,
        },
        Serializer,
    },
    validation::validators::DefaultValidator,
    vec::ArchivedVec,
    AlignedVec, Archive, CheckBytes, Serialize,
};

pub type DeferredBoxSerializer = CompositeSerializer<
    AlignedSerializer<AlignedVec>,
    FallbackScratch<HeapScratch<8192>, AllocScratch>,
>;

pub type DeferredBoxSerializerError = CompositeSerializerError<
    std::convert::Infallible,
    AllocScratchError,
    std::convert::Infallible,
>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("serializer: {0}")]
    Serializer(
        #[from]
        // TODO: why doesn't `<DeferredBoxSerializer as Fallible>::Error` work?
        DeferredBoxSerializerError,
    ),
    #[error("validation error: {0}")]
    Validate(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Serialize, Archive)]
#[archive(check_bytes)]
pub struct DeferredBox {
    bytes: AlignedVec,
}

impl DeferredBox {
    pub fn new<B, T>(data: B) -> Result<Self>
    where
        B: Borrow<T>,
        T: Serialize<DeferredBoxSerializer>,
    {
        let mut seri = DeferredBoxSerializer::default();
        seri.serialize_value(data.borrow())?;
        let mut vec = seri.into_serializer().into_inner();
        vec.shrink_to_fit();
        Ok(Self { bytes: vec })
    }
}

impl ArchivedDeferredBox {
    pub fn get<'a, T>(&'a self) -> Result<&'a T::Archived>
    where
        T: Archive,
        T::Archived: CheckBytes<DefaultValidator<'a>>,
    {
        rkyv::check_archived_root::<T>(self.bytes.as_slice())
            .map_err(|e| Error::Validate(format!("{e}")))
    }

    pub fn get_mut<'a, T>(self: Pin<&'a mut Self>) -> Result<Pin<&'a mut T::Archived>>
    where
        T: Archive,
        T::Archived: for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        let slice = self.bytes.as_slice();
        // TODO: https://github.com/rkyv/rkyv/issues/260
        rkyv::check_archived_root::<T>(slice)
            .map_err(|e| Error::Validate(format!("{e}")))?;

        let slice = self.pin_mut_bytes().pin_mut_slice();
        Ok(unsafe { rkyv::archived_root_mut::<T>(slice) })
    }
}

impl ArchivedDeferredBox {
    fn pin_mut_bytes(self: Pin<&mut Self>) -> Pin<&mut ArchivedVec<u8>> {
        unsafe { self.map_unchecked_mut(|s| &mut s.bytes) }
    }
}
