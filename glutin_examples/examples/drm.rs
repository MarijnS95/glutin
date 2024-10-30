use std::fs::{File, OpenOptions};
use std::os::fd::{AsFd, AsRawFd};

use drm::buffer::{Buffer, DrmFourcc};
use drm::control::Device as _;
use glutin::api::egl;
use glutin::api::egl::image::{DmaBufPlane, ImageBuffer};
use raw_window_handle::{DrmDisplayHandle, RawDisplayHandle};

struct Device(File);
impl AsFd for Device {
    fn as_fd(&self) -> std::os::unix::prelude::BorrowedFd<'_> {
        self.0.as_fd()
    }
}
impl AsRawFd for Device {
    fn as_raw_fd(&self) -> std::os::unix::prelude::RawFd {
        self.0.as_raw_fd()
    }
}
impl drm::Device for Device {}
impl drm::control::Device for Device {}

fn main() {
    let devices = egl::device::Device::query_devices().expect("Query EGL devices");
    for egl_device in devices {
        dbg!(&egl_device);
        dbg!(egl_device.drm_render_device_node_path());
        let Some(drm) = dbg!(egl_device.drm_device_node_path()) else {
            continue;
        };
        let fd = OpenOptions::new()
            .read(true)
            .write(true)
            .open(drm)
            .expect("Open DRM device with Read/Write permissions");

        let device = Device(fd);

        // https://registry.khronos.org/EGL/extensions/EXT/EGL_EXT_device_drm.txt:
        // Providing DRM_MASTER_FD is only to cover cases where EGL might fail to open
        // it itself.
        let rdh = RawDisplayHandle::Drm(DrmDisplayHandle::new(device.as_raw_fd()));

        let egl_display = unsafe { egl::display::Display::with_device(&egl_device, Some(rdh)) }
            .expect("Create EGL Display");
        dbg!(&egl_display);

        // TODO: bpp may be wrong
        let buf = device.create_dumb_buffer((10, 10), DrmFourcc::Xrgb8888, 8).unwrap();

        // TODO: No unsafe?
        let image = unsafe {
            egl_display.create_image(
                ImageBuffer::DmaBuf {
                    plane0: DmaBufPlane {
                        // TODO: Dropping fd after constructor is done?
                        // Or should it be kept open, which we need to signify with a lifetime?
                        fd: device.buffer_to_prime_fd(buf.handle(), 0).unwrap().as_fd(),
                        offset: 0,
                        pitch: buf.pitch() as i32,
                    },
                    width: buf.size().0 as i32,
                    height: buf.size().1 as i32,
                    drm_fourcc: buf.format() as _,
                    plane1: None,
                    plane2: None,
                },
                false,
            )
        }
        .unwrap();
        dbg!(image);
    }
}
