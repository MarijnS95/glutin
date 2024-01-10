//! Manual implementation of EGL bindings.
//!
//! This is necessary since `gl_generator` is unmaintaned and incapable of
//! generating bindings for some of the newer extensions.

pub type khronos_utime_nanoseconds_t = super::khronos_utime_nanoseconds_t;
pub type khronos_uint64_t = super::khronos_uint64_t;
pub type khronos_ssize_t = super::khronos_ssize_t;
pub type EGLNativeDisplayType = super::EGLNativeDisplayType;
pub type EGLNativePixmapType = super::EGLNativePixmapType;
pub type EGLNativeWindowType = super::EGLNativeWindowType;
pub type EGLint = super::EGLint;
pub type NativeDisplayType = super::EGLNativeDisplayType;
pub type NativePixmapType = super::EGLNativePixmapType;
pub type NativeWindowType = super::EGLNativeWindowType;

include!(concat!(env!("OUT_DIR"), "/egl_bindings.rs"));

// EGL_ANGLE_platform_angle - https://chromium.googlesource.com/angle/angle/+/HEAD/extensions/EGL_ANGLE_platform_angle.txt
pub const PLATFORM_ANGLE_ANGLE: super::EGLenum = 0x3202;
pub const PLATFORM_ANGLE_TYPE_ANGLE: super::EGLenum = 0x3203;
pub const PLATFORM_ANGLE_TYPE_VULKAN_ANGLE: super::EGLenum = 0x3450;
pub const PLATFORM_ANGLE_MAX_VERSION_MAJOR_ANGLE: super::EGLenum = 0x3204;
pub const PLATFORM_ANGLE_MAX_VERSION_MINOR_ANGLE: super::EGLenum = 0x3205;
pub const PLATFORM_ANGLE_DEBUG_LAYERS_ENABLED: super::EGLenum = 0x3451;
pub const PLATFORM_ANGLE_NATIVE_PLATFORM_TYPE_ANGLE: super::EGLenum = 0x348F;
pub const PLATFORM_ANGLE_TYPE_DEFAULT_ANGLE: super::EGLenum = 0x3206;
pub const PLATFORM_ANGLE_DEVICE_TYPE_HARDWARE_ANGLE: super::EGLenum = 0x320A;
pub const PLATFORM_ANGLE_DEVICE_TYPE_NULL_ANGLE: super::EGLenum = 0x345E;
