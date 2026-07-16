use std::sync::mpsc;
use std::sync::Arc;

use crate::core::sync::Mutex;

use super::decoder::VideoFrame;

pub struct VideoStream {
    pub width: u32,
    pub height: u32,
    pub fps_estimate: f32,
    pub duration_sec: f64,
    rx: Mutex<Option<mpsc::Receiver<Arc<VideoFrame>>>>,
}

impl PartialEq for VideoStream {
    fn eq(&self, _other: &Self) -> bool {
        false
    }
}

impl Eq for VideoStream {}

impl VideoStream {
    pub fn from_channel(
        rx: mpsc::Receiver<Arc<VideoFrame>>,
        width: u32,
        height: u32,
        fps_estimate: f32,
        duration_sec: f64,
    ) -> Arc<Self> {
        Arc::new(Self {
            width: width.max(1),
            height: height.max(1),
            fps_estimate,
            duration_sec,
            rx: Mutex::new(Some(rx)),
        })
    }

    pub fn take_receiver(&self) -> Option<mpsc::Receiver<Arc<VideoFrame>>> {
        self.rx.lock().ok().and_then(|mut g| g.take())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_frame() -> Arc<VideoFrame> {
        Arc::new(VideoFrame {
            width: 2,
            height: 2,
            rgba: Arc::from(vec![0u8; 16].into_boxed_slice()),
            pts_sec: 0.0,
        })
    }

    #[test]
    fn take_receiver_some_then_none() {
        let (_tx, rx) = mpsc::channel::<Arc<VideoFrame>>();
        let stream = VideoStream::from_channel(rx, 64, 64, 30.0, 1.0);
        assert!(stream.take_receiver().is_some());
        assert!(stream.take_receiver().is_none());
    }

    #[test]
    fn sender_delivers_frames() {
        let (tx, rx) = mpsc::channel::<Arc<VideoFrame>>();
        let stream = VideoStream::from_channel(rx, 2, 2, 30.0, 0.0);
        let rx = stream.take_receiver().expect("rx");
        tx.send(dummy_frame()).expect("send");
        let f = rx.recv().expect("recv");
        assert_eq!(f.width, 2);
        assert_eq!(f.height, 2);
    }

    #[test]
    fn dimensions_clamped_to_at_least_one() {
        let (_tx, rx) = mpsc::channel::<Arc<VideoFrame>>();
        let stream = VideoStream::from_channel(rx, 0, 0, 0.0, 0.0);
        assert_eq!(stream.width, 1);
        assert_eq!(stream.height, 1);
    }
}
