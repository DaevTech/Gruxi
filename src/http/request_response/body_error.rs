use std::error::Error as StdError;

// Unified body error type for streaming responses.
//
// We need one error type that can represent both `hyper::Error` (proxying upstream)
// and `std::io::Error` (streaming local files).
pub type BodyError = Box<dyn StdError + Send + Sync>;

pub fn box_err<E>(err: E) -> BodyError
where
    E: StdError + Send + Sync + 'static,
{
    Box::new(err)
}
