//! Audio primitives shared by Aède, Phémios and FlacCompagnon.
//!
//! No catalogue, account, network, history or application dependency. Decoding,
//! queueing and device output are explicit features. The minimal player applies
//! only volume and format adaptation; it has no normalization, EQ, crossfade or
//! bit-perfect route. Rich applications retain their transport policy.
#[cfg(feature = "dsp")]
pub use aede_dsp as dsp;
#[cfg(feature = "decode")]
pub mod decode;
#[cfg(feature = "decode")]
pub mod dsd;
#[cfg(feature = "dsp")]
pub mod format;
#[cfg(feature = "decode")]
pub mod native;
#[cfg(feature = "decode")]
pub mod pcm_wave;
#[cfg(feature = "queue")]
pub mod queue;
#[cfg(feature = "queue")]
pub mod transport;
#[cfg(feature = "queue")]
pub use rtrb as ring;
#[cfg(feature = "output")]
pub mod output;

#[cfg(all(test, feature = "decode"))]
mod test_support;
