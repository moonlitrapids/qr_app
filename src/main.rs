//! Runs the application and handles communication between the backend and ui.
use std::{rc, sync::mpsc::Sender};

use log::*;
use qr_app::{BackgroundThreadCommunicator, PixelMapResult, QrGenerationRequest};
use slint::{ComponentHandle, Image};

slint::include_modules!();

fn main() {
    start_application();
}

fn start_application() {
    let window = MainWindow::new().unwrap();
    let global_callbacks: NativeCallbacks = window.global();

    // Creates a new background thread worker with a callback function to update the image
    let window_weak = window.as_weak();
    let thread_com = BackgroundThreadCommunicator::new_thread(Box::new(move |pixels| {
        // After background thread is done work, have the main thread update the qr code image shown
        window_weak.upgrade_in_event_loop(|window: MainWindow| window.update_image(pixels))
    }));

    // Sets the callback for generating a new qr code
    let weak_sender = thread_com.get_weak_sender();
    global_callbacks
        .on_generate_qr_code(move |str, ec| generate_qr_code(weak_sender.clone(), str, ec));

    // Run window
    window.run().unwrap();
    // Clean up
    let _ = thread_com.stop_sender().join();
}

/// Generates the qr code by sending the information to the background thread
fn generate_qr_code(
    weak_sender: rc::Weak<Sender<QrGenerationRequest>>,
    shared_data: slint::SharedString,
    ec_level: EcLevel,
) {
    let data = shared_data.as_str().to_string();
    let ecl = ec_level.to_qr_enum();

    if let Some(sender) = weak_sender.upgrade() {
        // Set to background thread. If send errors, logs it
        if let Err(e) = sender.send(QrGenerationRequest {
            data,
            correction_level: ecl,
        }) {
            error!("Send error while sending: {}", e.to_string())
        }
    } else {
        // Sender already dropped
        warn!("Tried sending but thread sender doesn't exist!")
    }
}

impl MainWindow {
    /// Receives the image data or error and displays it
    fn update_image(&self, pixmap_result: PixelMapResult) {
        let state = self.global::<State>();

        match pixmap_result {
            Ok(pix_buffer) => {
                state.set_img(Image::from_rgba8_premultiplied(pix_buffer));
                state.set_image_status(ImageStatus::Image);
            }
            Err(str) => {
                state.set_err_msg(Into::into(str));
                state.set_image_status(ImageStatus::Error);
            }
        };
    }
}

impl EcLevel {
    /// Converts slint's `EcLevel` to `fast_qr::ECL`
    pub fn to_qr_enum(&self) -> Option<fast_qr::ECL> {
        match self {
            EcLevel::Default => None,
            EcLevel::L => Some(fast_qr::ECL::L),
            EcLevel::M => Some(fast_qr::ECL::M),
            EcLevel::Q => Some(fast_qr::ECL::Q),
            EcLevel::H => Some(fast_qr::ECL::H),
        }
    }
}
