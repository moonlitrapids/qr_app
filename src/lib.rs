//! Backend for qr code generation in a second thread
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use fast_qr::convert::image::ImageBuilder;
use fast_qr::convert::{Builder, Shape};
use fast_qr::qr::QRBuilder;
use fast_qr::{QRCode, ECL};
use log::error;
use slint::{EventLoopError, Rgba8Pixel, SharedPixelBuffer};

#[derive(Debug)]
pub struct QrGenerationRequest {
    pub data: String,
    pub correction_level: Option<ECL>,
}

#[derive(Debug)]
pub struct BackgroundThreadCommunicator {
    handle: JoinHandle<()>,
    sender: Rc<Sender<QrGenerationRequest>>,
}

pub type PixelMapResult = Result<SharedPixelBuffer<Rgba8Pixel>, String>;
pub type DynPixmapCallbackFn = dyn Fn(PixelMapResult) -> Result<(), EventLoopError> + Send;

impl BackgroundThreadCommunicator {
    pub fn new_thread(callback_function: Box<DynPixmapCallbackFn>) -> Self {
        let (tx, rx) = mpsc::channel();
        let handle = BackgroundThread::spawn(rx, callback_function);
        BackgroundThreadCommunicator {
            handle,
            sender: Rc::new(tx),
        }
    }

    /// Return will be valid until `self` is dropped or `stop_sender()` is called.
    pub fn get_weak_sender(&self) -> std::rc::Weak<Sender<QrGenerationRequest>> {
        Rc::downgrade(&self.sender)
    }

    pub fn stop_sender(self) -> JoinHandle<()> {
        drop(self.sender);
        self.handle
    }
}

struct BackgroundThread {
    receiver: Receiver<QrGenerationRequest>,
    display_callback: Box<DynPixmapCallbackFn>,
}

impl BackgroundThread {
    fn spawn(
        rx: Receiver<QrGenerationRequest>,
        callback_function: Box<DynPixmapCallbackFn>,
    ) -> JoinHandle<()> {
        thread::spawn(|| {
            BackgroundThread {
                receiver: rx,
                display_callback: callback_function,
            }
            .work_when_available();
        })
    }

    fn work_when_available(self) {
        while let Ok(mut qr_gen_req) = self.receiver.recv() {
            // Get latest, throw out old
            while let Ok(new_request) = self.receiver.try_recv() {
                qr_gen_req = new_request
            }
            let image_result = new_qr_code_image(qr_gen_req).map(|code| {
                ImageBuilder::default()
                    .shape(Shape::Square)
                    // .background_color([255, 255, 255, 0])
                    .to_pixmap(&code)
            });

            // Convert Pixmap to SharedPixelBuffer
            let pix_buffer_result = image_result.map(|pixmap| {
                SharedPixelBuffer::clone_from_slice(pixmap.data(), pixmap.width(), pixmap.height())
            });

            // Call callback. If it returns an error, log it.
            if let Err(e) = (*self.display_callback)(pix_buffer_result) {
                error!("Error while calling display callback: {}", e.to_string());
            };
        }
    }
}

fn new_qr_code_image(settings: QrGenerationRequest) -> Result<QRCode, String> {
    let mut builder = QRBuilder::new(settings.data);
    if let Some(correction_level) = settings.correction_level {
        builder.ecl(correction_level);
    }

    let qr_code = builder.build().map_err(|err| err.to_string())?;
    Ok(qr_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_test_01() {
        let string = "Test data 123".to_string();

        let bt_comm = BackgroundThreadCommunicator::new_thread(Box::new(|spb_result| {
            spb_result.expect("QRCode generation returned error!");
            Ok(())
        }));

        let weak_sender = bt_comm.get_weak_sender();

        weak_sender
            .upgrade()
            .expect("Error while upgrading weak sender")
            .send(QrGenerationRequest {
                data: string,
                correction_level: None,
            })
            .expect("Error while sending!");

        let join_handle = bt_comm.stop_sender();

        // Sender should be dropped
        assert!(weak_sender.upgrade().is_none());

        join_handle
            .join()
            .expect("Background thread panicked before joining!");
    }
}
