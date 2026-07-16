use std::sync::mpsc;
use std::sync::Arc;

use crate::core::sync::Mutex;

pub struct AudioStream {
    pub sample_rate: u32,
    pub channels: u16,
    rx: Mutex<Option<mpsc::Receiver<Vec<f32>>>>,
}

impl PartialEq for AudioStream {
    fn eq(&self, _other: &Self) -> bool {
        false
    }
}

impl Eq for AudioStream {}

impl AudioStream {
    pub(crate) fn new(rx: mpsc::Receiver<Vec<f32>>, sample_rate: u32, channels: u16) -> Arc<Self> {
        Self::from_channel(rx, sample_rate, channels)
    }

    pub fn from_channel(
        rx: mpsc::Receiver<Vec<f32>>,
        sample_rate: u32,
        channels: u16,
    ) -> Arc<Self> {
        Arc::new(Self {
            sample_rate,
            channels: channels.max(1),
            rx: Mutex::new(Some(rx)),
        })
    }

    pub fn take_receiver(&self) -> Option<mpsc::Receiver<Vec<f32>>> {
        self.rx.lock().ok().and_then(|mut g| g.take())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_receiver_some_then_none() {
        let (_tx, rx) = mpsc::channel::<Vec<f32>>();
        let stream = AudioStream::new(rx, 48_000, 1);

        let first = stream.take_receiver();
        assert!(first.is_some(), "первый take должен вернуть Some");

        let second = stream.take_receiver();
        assert!(second.is_none(), "повторный take должен вернуть None");
    }

    #[test]
    fn sender_drop_closes_receiver() {
        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        let stream = AudioStream::new(rx, 48_000, 1);
        let rx = stream.take_receiver().expect("первый take");

        drop(tx);
        match rx.recv() {
            Ok(_) => panic!("ожидаем RecvError после drop sender'а"),
            Err(_) => {}
        }
    }

    #[test]
    fn metadata_preserved() {
        let (_tx, rx) = mpsc::channel::<Vec<f32>>();
        let stream = AudioStream::new(rx, 44_100, 2);
        assert_eq!(stream.sample_rate, 44_100);
        assert_eq!(stream.channels, 2);
    }

    #[test]
    fn channels_clamped_to_at_least_one() {
        let (_tx, rx) = mpsc::channel::<Vec<f32>>();
        let stream = AudioStream::new(rx, 48_000, 0);
        assert_eq!(stream.channels, 1, "channels=0 должен клампиться к 1");
    }
}
