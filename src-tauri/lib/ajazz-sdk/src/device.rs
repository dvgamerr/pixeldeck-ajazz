use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hidapi::{HidApi, HidError};
use image::DynamicImage;

#[cfg(not(target_os = "windows"))]
use hidapi::HidDevice as DeviceHandle;
#[cfg(target_os = "windows")]
use crate::windows_hid::WindowsHidDevice as DeviceHandle;

use crate::images::{
    WriteImageParameters, convert_image, convert_image_with_format,
    convert_image_with_format_max_size,
};
use crate::info::Kind;
use crate::hid::is_supported_interface;
use crate::protocol::{codes, extract_string, request, AjazzProtocolParser, AjazzRequestBuilder};
use crate::input_state::handle_input_state_change;
use crate::{AjazzError, AjazzInput, DeviceState, Event};

/// Interface for an Ajazz device
pub struct Ajazz {
    /// Kind of the device
    kind: Kind,
    /// Connected HIDDevice
    hid: DeviceHandle,
    /// Temporarily cache the image before sending it to the device
    image_cache: RwLock<Vec<ImageCache>>,
    /// Device needs to be initialized
    initialized: AtomicBool,
}

struct ImageCache {
    key: u8,
    image_data: Vec<u8>,
}

/// Static functions of the struct
impl Ajazz {
    /// Attempts to connect to the device
    /// If the connection fails, it will retry up to `attempts` times
    /// If the connection fails after `attempts` retries, it will return an last error
    pub fn connect_with_retries(
        hidapi: &HidApi,
        kind: Kind,
        serial: &str,
        attempts: u8,
    ) -> Result<Ajazz, AjazzError> {
        if attempts == 0 {
            return Err(AjazzError::UnsupportedOperation);
        }

        let mut last_error = None;
        for _ in 0..attempts {
            match Self::try_connect(hidapi, kind, serial) {
                Ok(device) => return Ok(device),
                Err(e) => {
                    std::thread::sleep(Duration::from_millis(100));
                    last_error = Some(e);
                    continue;
                }
            }
        }

        Err(last_error.expect("error must never be empty at this point"))
    }

    /// Attempts to connect to the device
    pub fn connect(hidapi: &HidApi, kind: Kind, serial: &str) -> Result<Ajazz, AjazzError> {
        Self::try_connect(hidapi, kind, serial)
    }

    // Internal function to connect to the device
    fn try_connect(hidapi: &HidApi, kind: Kind, serial: &str) -> Result<Ajazz, AjazzError> {
        let info = hidapi
            .device_list()
            .find(|device| {
                device.vendor_id() == kind.vendor_id()
                    && device.product_id() == kind.product_id()
                    && device.serial_number() == Some(serial)
                    && is_supported_interface(device, kind)
            })
            .ok_or(AjazzError::DeviceNotFound)?;
        #[cfg(not(target_os = "windows"))]
        let device = info.open_device(hidapi)?;
        #[cfg(target_os = "windows")]
        let device = DeviceHandle::open(info.path())?;

        Ok(Ajazz {
            kind,
            hid: device,
            image_cache: RwLock::new(vec![]),
            initialized: false.into(),
        })
    }
}

/// Instance methods of the struct
impl Ajazz {
    /// Returns kind of the Ajazz device
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// Returns manufacturer string of the device
    pub fn manufacturer(&self) -> Result<String, AjazzError> {
        Ok(self
            .hid
            .get_manufacturer_string()?
            .unwrap_or_else(|| "Unknown".to_string()))
    }

    /// Returns product string of the device
    pub fn product(&self) -> Result<String, AjazzError> {
        Ok(self
            .hid
            .get_product_string()?
            .unwrap_or_else(|| "Unknown".to_string()))
    }

    /// Returns serial number of the device
    pub fn serial_number(&self) -> Result<String, AjazzError> {
        let serial = self.hid.get_serial_number_string()?;
        match serial {
            Some(serial) => {
                if serial.is_empty() {
                    Ok("Unknown".to_string())
                } else {
                    Ok(serial)
                }
            }
            None => Ok("Unknown".to_string()),
        }
    }

    /// Returns firmware version of the device
    pub fn firmware_version(&self) -> Result<String, AjazzError> {
        let mut buff = request::FEATURE_REPORT_VERSION.clone();
        self.hid.get_feature_report(buff.as_mut_slice())?;

        let version = extract_string(&buff[0..])?;
        Ok(version)
    }

    /// Sleeps the device
    pub fn sleep(&self) -> Result<(), AjazzError> {
        self.initialize()?;

        let packet = self.kind.sleep_packet();
        self.hid.write(packet.as_slice())?;

        Ok(())
    }

    /// Make periodic events to the device, to keep it alive
    pub fn keep_alive(&self) -> Result<(), AjazzError> {
        self.initialize()?;

        let packet = self.kind.keep_alive_packet();
        self.hid.write(packet.as_slice())?;

        Ok(())
    }

    /// Returns device state reader for this device
    pub fn get_reader(self: &Arc<Self>) -> Arc<DeviceStateReader> {
        #[allow(clippy::arc_with_non_send_sync)]
        Arc::new(DeviceStateReader {
            device: self.clone(),
            states: Mutex::new(DeviceState {
                buttons: vec![false; self.kind.key_count() as usize],
                encoders: vec![false; self.kind.encoder_count() as usize],
            }),
        })
    }

    /// Shutdown the device
    pub fn shutdown(&self) -> Result<(), AjazzError> {
        self.initialize()?;

        let packet = self.kind.shutdown_packet();
        self.hid.write(packet.as_slice())?;

        let packet = self.kind.sleep_packet();
        self.hid.write(packet.as_slice())?;

        Ok(())
    }

    /// Reads input from the device
    pub fn read_input(&self, timeout: Option<Duration>) -> Result<AjazzInput, AjazzError> {
        self.initialize()?;

        let data = self.read_data(codes::INPUT_PACKET_LENGTH, timeout)?;
        self.kind.parse_input(&data)
    }

    /// Resets the device
    pub fn reset(&self) -> Result<(), AjazzError> {
        self.initialize()?;

        self.set_brightness(100)?;
        self.clear_all_button_images()
    }

    /// Sets brightness of the device, value range is 0 - 100
    pub fn set_brightness(&self, percent: u8) -> Result<(), AjazzError> {
        self.initialize()?;

        let buf = self.kind.brightness_packet(percent);
        self.hid.write(buf.as_slice())?;

        Ok(())
    }

    /// Sets button's image to blank, changes must be flushed with `.flush()` before
    /// they will appear on the device!
    pub fn clear_button_image(&self, key: u8) -> Result<(), AjazzError> {
        self.initialize()?;

        let od_key: u8 = self.kind.opendeck_to_device_key(key)?;
        let packet = self.kind.clear_button_image_packet(od_key);
        self.hid.write(packet.as_slice())?;

        Ok(())
    }

    /// Flushes the button's image to the device
    pub fn flush(&self) -> Result<(), AjazzError> {
        self.flush_inner(false)
    }

    pub(crate) fn flush_batch(&self) -> Result<(), AjazzError> {
        self.flush_inner(true)
    }

    fn flush_inner(&self, force: bool) -> Result<(), AjazzError> {
        self.initialize()?;

        let is_empty = {
            let images = self
                .image_cache
                .read()
                .map_err(|_| AjazzError::PoisonError)?;

            images.is_empty()
        };

        if is_empty {
            if force {
                let packet = self.kind.flush_packet();
                self.hid.write(packet.as_slice())?;
            }
            return Ok(());
        }

        let mut images = self
            .image_cache
            .write()
            .map_err(|_| AjazzError::PoisonError)?;

        let flush_each_image = should_flush_each_image(self.kind);
        for image in images.iter() {
            self.write_key_image(image.key, &image.image_data)?;
            if flush_each_image {
                let packet = self.kind.flush_packet();
                self.hid.write(packet.as_slice())?;
            }
        }

        if !flush_each_image {
            let packet = self.kind.flush_packet();
            self.hid.write(packet.as_slice())?;
        }
        images.clear();

        Ok(())
    }

    /// Sets blank images to every button, changes must be flushed with `.flush()` before
    /// they will appear on the device!
    pub fn clear_all_button_images(&self) -> Result<(), AjazzError> {
        self.initialize()?;
        self.clear_button_image(codes::CMD_CLEAR_ALL)?;

        if self.kind.is_v2_api() {
            // Mirabox "v2" requires flush to commit clearing the background
            let packet = self.kind.flush_packet();
            self.hid.write(packet.as_slice())?;
        }

        Ok(())
    }

    /// Sets specified button's image, changes must be flushed with `.flush()` before
    /// they will appear on the device!
    pub fn set_button_image_data(&self, key: u8, image_data: &[u8]) -> Result<(), AjazzError> {
        self.initialize()?;
        self.write_image_to_cache(key, image_data)?;
        Ok(())
    }

    /// Sets specified button's image, changes must be flushed with `.flush()` before
    /// they will appear on the device!
    pub fn set_button_image(&self, key: u8, image: DynamicImage) -> Result<(), AjazzError> {
        self.initialize()?;
        let image_data = convert_image(self.kind, image)?;
        self.write_image_to_cache(key, &image_data)?;
        Ok(())
    }

    /// Sets an image on one of the four zones of the AKP05E_552A touchscreen strip.
    pub fn set_touch_zone_image(
        &self,
        touch: u8,
        image: DynamicImage,
    ) -> Result<(), AjazzError> {
        self.initialize()?;
        if self.kind != Kind::Akp05E552A || touch >= self.kind.touchpoint_count() {
            return Err(AjazzError::InvalidKeyIndex(touch));
        }

        let format = self
            .kind
            .touch_zone_image_format()
            .ok_or(AjazzError::UnsupportedOperation)?;
        let image_data = convert_image_with_format(format, image)?;
        self.write_native_image_to_cache(touch, &image_data)
    }

    /// Clears one of the four zones of the AKP05E_552A touchscreen strip.
    pub fn clear_touch_zone_image(&self, touch: u8) -> Result<(), AjazzError> {
        self.initialize()?;
        if self.kind != Kind::Akp05E552A || touch >= self.kind.touchpoint_count() {
            return Err(AjazzError::InvalidKeyIndex(touch));
        }

        let packet = self.kind.clear_button_image_packet(touch);
        self.hid.write(packet.as_slice())?;
        Ok(())
    }

    /// Set logo image
    pub fn set_logo_image(&self, image: DynamicImage) -> Result<(), AjazzError> {
        self.initialize()?;

        if self.kind.boot_logo_size().is_none() {
            return Err(AjazzError::UnsupportedOperation);
        }

        let image_data = if self.kind.is_v2_api() {
            convert_image_with_format_max_size(
                self.kind.logo_image_format(),
                image,
                u16::MAX as usize,
            )?
        } else {
            convert_image_with_format(self.kind.logo_image_format(), image)?
        };
        if self.kind.is_v2_api() && image_data.len() > u16::MAX as usize {
            return Err(AjazzError::ImageDataTooLarge(
                image_data.len(),
                u16::MAX as usize,
            ));
        }
        self.hid
            .write(self.kind.logo_image_packet(&image_data).as_slice())?;

        if self.kind == Kind::Akp05E552A {
            // This v2 firmware commits a boot logo when STP follows the image payload
            // and does not emit the legacy ACK report used by older devices.
            self.write_image_data_reports(
                &image_data,
                WriteImageParameters::for_kind(self.kind),
            )?;
            self.hid.write(self.kind.flush_packet().as_slice())?;
        } else {
            self.hid.write(self.kind.flush_packet().as_slice())?;
            self.write_image_data_reports(
                &image_data,
                WriteImageParameters::for_kind(self.kind),
            )?;
            self.assert_write_complete()?;
        }

        Ok(())
    }

    /// Initializes the device
    fn initialize(&self) -> Result<(), AjazzError> {
        if self.initialized.load(Ordering::Acquire) {
            return Ok(());
        }

        self.initialized.store(true, Ordering::Release);

        let packet = self.kind.initialize_packet();
        self.hid.write(packet.as_slice())?;

        Ok(())
    }

    /// Writes image data to Ajazz device, changes must be flushed with `.flush()` before
    /// they will appear on the device!
    fn write_image_to_cache(&self, key: u8, image_data: &[u8]) -> Result<(), AjazzError> {
        let od_key: u8 = self.kind.opendeck_to_device_key(key)?;
        self.write_native_image_to_cache(od_key, image_data)
    }

    pub(crate) fn write_native_image_to_cache(
        &self,
        key: u8,
        image_data: &[u8],
    ) -> Result<(), AjazzError> {
        let cache_entry = ImageCache {
            key,
            image_data: image_data.to_vec(), // Convert &[u8] to Vec<u8>
        };

        let Ok(mut image_cache) = self.image_cache.write() else {
            return Err(AjazzError::PoisonError);
        };

        image_cache.push(cache_entry);

        Ok(())
    }

    /// Writes key image to the device
    fn write_key_image(&self, key: u8, image_data: &[u8]) -> Result<(), AjazzError> {
        let packet = self.kind.key_image_announce_packet(key, image_data);
        self.hid.write(packet.as_slice())?;

        self.write_image_data_reports(image_data, WriteImageParameters::for_kind(self.kind))?;
        Ok(())
    }

    fn write_image_data_reports(
        &self,
        image_data: &[u8],
        parameters: WriteImageParameters,
    ) -> Result<(), AjazzError> {
        let image_report_length = parameters.image_report_length;
        let image_report_payload_length = parameters.image_report_payload_length;

        let mut page_number = 0;
        let mut bytes_remaining = image_data.len();

        while bytes_remaining > 0 {
            let this_length = bytes_remaining.min(image_report_payload_length);
            let bytes_sent = page_number * image_report_payload_length;

            let mut buf: Vec<u8> = vec![0x00];
            buf.extend(&image_data[bytes_sent..bytes_sent + this_length]);
            buf.extend(vec![0x00; image_report_length - buf.len()]);

            self.hid.write(buf.as_slice())?;
            bytes_remaining -= this_length;
            page_number += 1;
        }

        Ok(())
    }

    fn assert_write_complete(&self) -> Result<(), AjazzError> {
        let deadline = Instant::now() + Duration::from_millis(1000);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(AjazzError::NoAck);
            }

            let data = self.read_data(codes::INPUT_PACKET_LENGTH, Some(remaining))?;
            if data.is_empty() {
                return Err(AjazzError::NoAck);
            }
            if self.kind.is_ack_ok(&data) {
                return Ok(());
            }
        }
    }

    /// Reads data from [HidDevice]. Blocking mode is used if timeout is specified
    fn read_data(
        &self,
        length: usize,
        timeout: Option<Duration>,
    ) -> Result<Vec<u8>, HidError> {
        self.hid.set_blocking_mode(timeout.is_some())?;

        let mut buf = vec![0u8; length];

        let size = match timeout {
            Some(timeout) => self
                .hid
                .read_timeout(buf.as_mut_slice(), timeout.as_millis() as i32),
            None => self.hid.read(buf.as_mut_slice()),
        }?;

        buf.truncate(size);
        Ok(buf)
    }
}

// The AKP05E_552A firmware accepts one complete image transaction at a time:
// BAT header, JPEG reports, then STP. Sending another BAT before STP eventually
// fills its output queue and makes Windows time out a pending HID write.
fn should_flush_each_image(kind: Kind) -> bool {
    kind == Kind::Akp05E552A
}

/// Button reader that keeps state of the Ajazz and returns events instead of full states
pub struct DeviceStateReader {
    device: Arc<Ajazz>,
    states: Mutex<DeviceState>,
}

impl DeviceStateReader {
    /// Reads states and returns updates
    pub fn read(&self, timeout: Option<Duration>) -> Result<Vec<Event>, AjazzError> {
        let input = self.device.read_input(timeout)?;
        let mut current_state = self.states.lock().map_err(|_| AjazzError::PoisonError)?;

        let updates = handle_input_state_change(input, &mut current_state)?;
        Ok(updates)
    }
}

#[cfg(test)]
mod tests {
    use super::should_flush_each_image;
    use crate::Kind;

    #[test]
    fn akp05e_commits_each_image_before_starting_the_next_one() {
        assert!(should_flush_each_image(Kind::Akp05E552A));
        assert!(!should_flush_each_image(Kind::Akp153));
    }
}
