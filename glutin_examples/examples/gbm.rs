use std::fs::{File, OpenOptions};
use std::num::NonZero;
use std::os::fd::AsFd;
use std::ptr::NonNull;
use std::time::Duration;

use anyhow::{Context as _, Result};
use drm::control::connector::State;
use drm::control::Device as _;
use drm::Device as _;
use gbm::{AsRaw as _, BufferObjectFlags, Device, Format};
use glutin::api::egl;
use glutin::config::{ConfigSurfaceTypes, ConfigTemplateBuilder};
use glutin::context::{ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentGlContext};
use glutin::prelude::GlDisplay;
use glutin::surface::{GlSurface as _, SurfaceAttributesBuilder, WindowSurface};
use glutin_examples::Renderer;
use raw_window_handle::{GbmDisplayHandle, GbmWindowHandle, RawDisplayHandle, RawWindowHandle};

struct DrmDevice(File);
impl drm::Device for DrmDevice {}
impl drm::control::Device for DrmDevice {}
impl AsFd for DrmDevice {
    fn as_fd(&self) -> std::os::unix::prelude::BorrowedFd<'_> {
        self.0.as_fd()
    }
}

// TODO: Replace with std::env::args() parsing?
const USE_SURFACE: bool = true;

fn main() -> Result<()> {
    let drm_files = std::fs::read_dir("/dev/dri/").context("Read /dev/dri/")?;
    for device in drm_files {
        let drm = device?;
        if drm.file_type()?.is_dir() {
            continue;
        }
        if drm.path().file_name().unwrap().to_str().unwrap().starts_with("renderD") {
            continue;
        }
        dbg!(&drm);
        let fd =
            DrmDevice(OpenOptions::new().read(true).write(true).open(drm.path()).context("open")?);
        // fd.release_master_lock()?;
        dbg!(fd.get_driver()?);
        let drm_gbm = Device::new(fd).context("Create GBM device")?;
        dbg!(drm_gbm.backend_name());

        // TODO: It doesn't matter which method we use since we checked that this is our
        // GBM device, but theoretically reading the drm_device_file() requires
        // the EGL_EXT_device_drm extension to be present which is not necessary
        // if just doing GBM.

        let egl_display = unsafe {
            egl::display::Display::new(RawDisplayHandle::Gbm(GbmDisplayHandle::new(
                NonNull::new(drm_gbm.as_raw().cast_mut().cast()).context("Null GBM device")?,
            )))
        }
        .context("Create EGL Display")?;
        dbg!(&egl_display);

        // ----------- START ---------
        let rsc = drm_gbm.resource_handles().context("resource_handles")?;
        let conn = rsc
            .connectors()
            .iter()
            .map(|&c| drm_gbm.get_connector(c, false).unwrap())
            .find(|c| c.state() == State::Connected)
            .context("No connected connector")?;
        // dbg!(&conn);

        // TODO: Not a requirement that it is the _current_ encoder for the connector?
        let possible_crtcs = drm_gbm.get_encoder(conn.current_encoder().unwrap())?.possible_crtcs();
        dbg!(possible_crtcs);

        let &crtc = rsc.filter_crtcs(possible_crtcs).first().unwrap();
        dbg!(crtc);
        let mode = *drm_gbm.get_modes(conn.handle())?.first().unwrap();
        // let mode = *modes.iter().find(|m| m.vrefresh() == 240).unwrap();
        dbg!(&mode);
        let (width, height) = mode.size();
        let (width, height) = (width as u32, height as u32);
        // --------- TEMP END ------------

        let config = unsafe {
            egl_display.find_configs(
                ConfigTemplateBuilder::new()
                    // .with_alpha_size(0)
                    .with_surface_type(if USE_SURFACE {
                        ConfigSurfaceTypes::WINDOW
                    } else {
                        // TODO: Unknown what config to pick when using GL_OES_image to bind an
                        // EGLImage as texture or renderbuffer to a framebuffer.
                        ConfigSurfaceTypes::empty()
                    })
                    .build(),
            )
        }?
        .next()
        .context("Find config")?;
        dbg!(&config);

        let context = unsafe {
            egl_display.create_context(&config, &ContextAttributesBuilder::new().build(None))
        }?;
        dbg!(&context);

        enum RenderTarget {
            Surface { surface: gbm::Surface<()>, egl_surface: egl::surface::Surface<WindowSurface> },
            Image { bo: gbm::BufferObject<()>, image: egl::image::Image },
        }

        // TODO: There's a third method: importing an existing EGLImage into a gbm_bo
        let (context, target) = if USE_SURFACE {
            let surface = drm_gbm
                .create_surface::<()>(
                    width,
                    height,
                    Format::Xrgb8888,
                    BufferObjectFlags::SCANOUT | BufferObjectFlags::RENDERING,
                )
                .context("create_surface")?;
            dbg!(&surface);

            let egl_surface = unsafe {
                egl_display.create_window_surface(
                    &config,
                    &SurfaceAttributesBuilder::<WindowSurface>::new().build(
                        RawWindowHandle::Gbm(GbmWindowHandle::new(
                            NonNull::new(surface.as_raw().cast_mut().cast()).unwrap(),
                        )),
                        NonZero::new_unchecked(width),
                        NonZero::new_unchecked(height),
                    ),
                )
            }
            .context("create_window_surface")?;

            dbg!(&egl_surface);

            let context = context.make_current(&egl_surface)?;

            (context, RenderTarget::Surface { surface, egl_surface })
        } else {
            let bo = drm_gbm
                .create_buffer_object::<()>(
                    width,
                    height,
                    Format::Xrgb8888,
                    BufferObjectFlags::SCANOUT | BufferObjectFlags::RENDERING,
                )
                .context("create_buffer_object")?;
            dbg!(&bo);

            let image = unsafe {
                egl_display.create_image(
                    egl::image::ImageBuffer::NativePixmap { buffer: bo.as_raw().cast() },
                    true,
                )
            }
            .unwrap();
            dbg!(&image);

            let context = context.make_current_surfaceless()?;

            (context, RenderTarget::Image { bo, image })
        };

        let renderer = Renderer::new(&egl_display);
        if let RenderTarget::Image { bo: _, image } = &target {
            unsafe { renderer.set_framebuffer(image.as_raw()) };
        }
        renderer.resize(width as i32, height as i32);

        renderer.draw();
        unsafe { renderer.Finish() };
        let front_buffer = match &target {
            RenderTarget::Surface { surface, egl_surface } => {
                assert!(surface.has_free_buffers());
                assert!(unsafe { surface.lock_front_buffer() }.is_err());
                egl_surface.swap_buffers(&context).context("swap_buffers")?;
                &unsafe { surface.lock_front_buffer() }.context("lock_front_buffer")?
            },
            RenderTarget::Image { bo, image: _ } => bo,
        };
        dbg!(&front_buffer);
        // TODO: Signal a completion fence!
        let _context = context.make_not_current()?;

        // DRM is used to put the GBM surface on-screen.  This GBM surface could however
        // also have been received/retrieved from elsewhere, i.e. a compositor
        // which presents it via DRM.

        // TODO: Move
        let fb = drm_gbm.add_framebuffer(front_buffer, 24, 32).context("add_framebuffer")?;
        dbg!(fb);

        // drm_gbm.acquire_master_lock()?;

        drm_gbm
            .set_crtc(crtc, Some(fb), (0, 0), &[conn.handle()], Some(mode))
            .context("set_crtc")?;

        // drm_gbm.release_master_lock()?;

        std::thread::sleep(Duration::from_secs(4));

        // TODO: Throw this into an atexit/signal handler.
        // Quickly going to graphics and back to text makes sure the console works again
        // after having temporarily taken over
        const KDSETMODE: u64 = 0x4B3A;
        // const KDGETMODE: u64 = 0x4B3B;
        const KD_TEXT: i32 = 0x00;
        const KD_GRAPHICS: i32 = 0x01;
        unsafe {
            libc::ioctl(libc::STDIN_FILENO, KDSETMODE, KD_GRAPHICS);
            libc::ioctl(libc::STDIN_FILENO, KDSETMODE, KD_TEXT);
        }

        drm_gbm.destroy_framebuffer(fb).unwrap();
    }

    Ok(())
}
