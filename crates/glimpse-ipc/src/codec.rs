use std::io;

use tokio_util::{
    bytes::BytesMut,
    codec::{Decoder, Encoder, LinesCodec, LinesCodecError},
};

use crate::frame::Frame;

pub const MAX_LINE_BYTES: usize = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("a frame exceeded the {MAX_LINE_BYTES} byte limit")]
    TooLong,
    #[error("malformed frame")]
    Malformed(#[source] serde_json::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl From<LinesCodecError> for CodecError {
    fn from(error: LinesCodecError) -> Self {
        match error {
            LinesCodecError::MaxLineLengthExceeded => Self::TooLong,
            LinesCodecError::Io(error) => Self::Io(error),
        }
    }
}

pub struct FrameCodec(LinesCodec);

impl Default for FrameCodec {
    fn default() -> Self {
        Self(LinesCodec::new_with_max_length(MAX_LINE_BYTES))
    }
}

impl Decoder for FrameCodec {
    type Item = Frame;
    type Error = CodecError;

    // Both failures are terminal by policy. `LinesCodec` would resynchronise past an oversize line,
    // which is right for a log tailer and wrong here: resuming mid-stream leaves the two ends
    // disagreeing about what was delivered. Every caller drops the connection on `Err`.
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Frame>, CodecError> {
        let Some(line) = self.0.decode(src)? else {
            return Ok(None);
        };
        serde_json::from_str(&line)
            .map(Some)
            .map_err(CodecError::Malformed)
    }
}

impl Encoder<Frame> for FrameCodec {
    type Error = CodecError;

    fn encode(&mut self, frame: Frame, dst: &mut BytesMut) -> Result<(), CodecError> {
        let line = serde_json::to_string(&frame).map_err(CodecError::Malformed)?;
        self.0.encode(line, dst).map_err(CodecError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::Body;

    #[test]
    fn a_frame_survives_the_codec() {
        let frame = Frame {
            id: Some(1),
            body: Body::Get {
                topic: "audio.volume".into(),
            },
        };
        let mut buffer = BytesMut::new();
        let mut codec = FrameCodec::default();

        codec.encode(frame.clone(), &mut buffer).expect("encode");
        assert_eq!(codec.decode(&mut buffer).expect("decode"), Some(frame));
        assert!(codec.decode(&mut buffer).expect("decode").is_none());
    }

    #[test]
    fn an_oversize_line_is_an_error() {
        let mut buffer = BytesMut::from(vec![b'x'; MAX_LINE_BYTES + 1].as_slice());
        let error = FrameCodec::default()
            .decode(&mut buffer)
            .expect_err("too long");
        assert!(matches!(error, CodecError::TooLong));
    }

    #[test]
    fn a_malformed_line_is_an_error() {
        let mut buffer = BytesMut::from("{\"type\":\n".as_bytes());
        let error = FrameCodec::default()
            .decode(&mut buffer)
            .expect_err("malformed");
        assert!(matches!(error, CodecError::Malformed(_)));
    }
}
