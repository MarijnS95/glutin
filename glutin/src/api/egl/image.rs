//! Everything related to `EGLImage`.

use std::os::fd::{AsRawFd, BorrowedFd};

use glutin_egl_sys::egl;
use glutin_egl_sys::egl::types::{EGLClientBuffer, EGLImage};

use crate::context::Version;
use crate::error::{ErrorKind, Result};

use super::display::Display;

/// A wrapper for the `EGLImage`.
#[derive(Debug, Clone)]
pub struct Image {
    display: Display,
    pub(crate) raw: EGLImage,
}

impl Image {
    /// Returns the raw [`EGLImage`] pointer, typically used to pass to the
    /// `eglImageOES image` argument of `EGLImageTargetTexture2DOES()` or
    /// `EGLImageTargetRenderbufferStorageOES()` as defined by
    /// [`OES_EGL_image`].
    ///
    /// [`OES_EGL_image`]: https://registry.khronos.org/OpenGL/extensions/OES/OES_EGL_image.txt
    pub fn as_raw(&self) -> EGLImage {
        self.raw
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        assert_eq!(
            if self.display.inner.version >= Version::new(1, 5) {
                unsafe { self.display.inner.egl.DestroyImage(*self.display.inner.raw, self.raw) }
            } else {
                unsafe { self.display.inner.egl.DestroyImageKHR(*self.display.inner.raw, self.raw) }
            },
            egl::TRUE
        )
    }
}

/// Decribes a single [`dma_buf`] plane belonging to an [`ImageBuffer`].
///
/// [`dma_buf`]: https://docs.kernel.org/driver-api/dma-buf.html
#[derive(Clone, Debug)]
pub struct DmaBufPlane<'a> {
    /// The `dma_buf` file descriptor of the plane
    pub fd: BorrowedFd<'a>,
    /// The offset from the start of the `dma_buf` of the first sample in the
    /// plane, in bytex
    pub offset: i32,
    /// The number of bytes between the start of subsequent rows of samples in
    /// the plane. May have special meaning for non-linear formats.
    pub pitch: i32,
}

/// Description of various possible buffers and their parameters to pass to
/// [`Display::create_image()`].
// https://registry.khronos.org/EGL/sdk/docs/man/html/eglCreateImage.xhtml
#[derive(Clone, Debug)]
pub enum ImageBuffer<'a> {
    /// Import a buffer via [`egl::NATIVE_PIXMAP_KHR`]
    NativePixmap {
        /// Opaque handle of the native buffer that is going to be wrapped
        buffer: EGLClientBuffer,
    },
    /// Import a DMA-BUF via [`egl::LINUX_DMA_BUF_EXT`]
    // https://registry.khronos.org/EGL/extensions/EXT/EGL_EXT_image_dma_buf_import.txt
    DmaBuf {
        /// Main plane
        plane0: DmaBufPlane<'a>,
        /// Width of the image
        width: i32,
        /// Height of the image
        height: i32,
        /// The pixel format of the buffer, as specified by drm-fourcc
        // TODO: drm_fourcc crate?
        drm_fourcc: i32,
        /// First plane for multiplanar formats
        plane1: Option<DmaBufPlane<'a>>,
        /// Second plane for multiplanar formats
        plane2: Option<DmaBufPlane<'a>>,
    },
}

impl Display {
    /// Import an image from the specified [`ImageBuffer`] into an `EGL`
    /// [`Image`].
    ///
    /// These images can subsequently be bound in GL, see [`Image::as_raw()`]
    /// for more details.
    ///
    /// `image_preserved` is only supported on EGL 1.5, or EGL 1.2 when
    /// `EGL_KHR_image_base` is present [^1].
    ///
    /// [^1]: https://registry.khronos.org/EGL/extensions/KHR/EGL_KHR_image.txt
    ///
    /// # Safety
    /// `buffer` must be valid within the constraints of the underlying EGL API.
    pub unsafe fn create_image(
        &self,
        // There are currently no extensions (implemented here) that require a context
        // context: Option<&NotCurrentContext>,
        buffer: ImageBuffer<'_>,
        image_preserved: bool,
    ) -> Result<Image> {
        if self.inner.version < Version::new(1, 2) {
            return Err(
                ErrorKind::NotSupported("eglCreateImage() requires at least EGL 1.2").into()
            );
        }

        let mut attrib = vec![];

        // TODO: Pass GL_TEXTURE_LEVEL and GL_TEXTURE_ZOFFSET

        if image_preserved {
            if self.inner.version < Version::new(1, 5)
                && !self.inner.display_extensions.contains("EGL_KHR_image_base")
            {
                return Err(ErrorKind::NotSupported(
                    "The IMAGE_PRESERVED attribute is not supported before EGL 1.5 without \
                     EGL_KHR_image_base extension",
                )
                .into());
            }
            attrib.insert(0, egl::IMAGE_PRESERVED as _);
            attrib.insert(1, egl::TRUE as _);
        }

        let (buffer, target, ctx) = match buffer {
            ImageBuffer::NativePixmap { buffer } => {
                if self.inner.version < Version::new(1, 5)
                    && !(self.inner.display_extensions.contains("EGL_KHR_image")
                        || self.inner.display_extensions.contains("EGL_KHR_image_base")
                            && self.inner.display_extensions.contains("EGL_KHR_native_pixmap"))
                {
                    return Err(ErrorKind::NotSupported(
                        "NativePixmap requires EGL 1.5, or EGL 1.2 with EGL_KHR_image or \
                         EGL_KHR_image_base and EGL_KHR_native_pixmap",
                    )
                    .into());
                }

                (buffer, egl::NATIVE_PIXMAP_KHR, egl::NO_CONTEXT)
            },
            ImageBuffer::DmaBuf { plane0, width, height, drm_fourcc, plane1, plane2 } => {
                // XXX: We're assuming that EGL 1.5 subsumes the requirement for
                // EGL_KHR_image_base.
                if !self.inner.display_extensions.contains("EGL_EXT_image_dma_buf_import") {
                    return Err(ErrorKind::NotSupported(
                        "EGL_EXT_image_dma_buf_import is not supported",
                    )
                    .into());
                }
                if self.inner.version < Version::new(1, 5)
                    && !self.inner.display_extensions.contains("EGL_KHR_image_base")
                {
                    return Err(ErrorKind::NotSupported(
                        "EGL_EXT_image_dma_buf_import requires EGL 1.5, or EGL 1.2 with \
                         EGL_KHR_image_base",
                    )
                    .into());
                }
                attrib.push(egl::WIDTH as _);
                attrib.push(width);
                attrib.push(egl::HEIGHT as _);
                attrib.push(height);

                attrib.push(egl::LINUX_DRM_FOURCC_EXT as _);
                attrib.push(drm_fourcc);

                attrib.push(egl::DMA_BUF_PLANE0_FD_EXT as _);
                attrib.push(plane0.fd.as_raw_fd());
                attrib.push(egl::DMA_BUF_PLANE0_OFFSET_EXT as _);
                attrib.push(plane0.offset);
                attrib.push(egl::DMA_BUF_PLANE0_PITCH_EXT as _);
                attrib.push(plane0.pitch);

                if let Some(plane1) = plane1 {
                    attrib.push(egl::DMA_BUF_PLANE1_FD_EXT as _);
                    attrib.push(plane1.fd.as_raw_fd());
                    attrib.push(egl::DMA_BUF_PLANE1_OFFSET_EXT as _);
                    attrib.push(plane1.offset);
                    attrib.push(egl::DMA_BUF_PLANE1_PITCH_EXT as _);
                    attrib.push(plane1.pitch);
                }

                if let Some(plane2) = plane2 {
                    attrib.push(egl::DMA_BUF_PLANE2_FD_EXT as _);
                    attrib.push(plane2.fd.as_raw_fd());
                    attrib.push(egl::DMA_BUF_PLANE2_OFFSET_EXT as _);
                    attrib.push(plane2.offset);
                    attrib.push(egl::DMA_BUF_PLANE2_PITCH_EXT as _);
                    attrib.push(plane2.pitch);
                }

                // XXX: YUV attributes

                (std::ptr::null(), egl::LINUX_DMA_BUF_EXT, egl::NO_CONTEXT)
            },
        };

        attrib.push(egl::NONE as _);

        let image = if self.inner.version >= Version::new(1, 5) {
            let attrib = attrib.into_iter().map(|a| a as _).collect::<Vec<_>>();
            unsafe {
                self.inner.egl.CreateImage(*self.inner.raw, ctx, target, buffer, attrib.as_ptr())
            }
        } else {
            unsafe {
                self.inner.egl.CreateImageKHR(*self.inner.raw, ctx, target, buffer, attrib.as_ptr())
            }
        };

        super::check_error().map(|()| Image { display: self.clone(), raw: image })
    }
}
