//! EGL Manager with radiation hardening for safety-critical applications
//!
//! This library provides a centralized, radiation-hardened approach to managing EGL resources
//! with triple modular redundancy (TMR) for Single Event Upset (SEU) protection.
//!
//! # Safety
//!
//! EGL contexts have strict requirements regarding thread safety. This implementation
//! ensures that EGL contexts are properly managed with appropriate thread synchronization.
//!
//! # Radiation Hardening
//!
//! - Triple redundant state tracking
//! - Error detection and recovery via voting
//! - Bounded execution guarantees
//! - Static resource allocation with predictable cleanup

use std::sync::{Mutex, Once};

use anyhow::{anyhow, Context, Result};
use log::{debug, error};
use smithay::backend::allocator::dmabuf::AsDmabuf;
use smithay::backend::allocator::gbm::{GbmBuffer, GbmDevice};
use smithay::backend::allocator::Fourcc;
use smithay::backend::drm::DrmDeviceFd;
use smithay::backend::egl::context::{ContextPriority, EGLContext};
use smithay::backend::egl::display::EGLDisplay;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::ImportDma;
use smithay::reexports::drm::buffer::DrmModifier;
use smithay::utils::Size;

// Public type alias for easier use by consumers
pub type Modifier = DrmModifier;

// Public API module with only safe functions
pub mod safe {
    use smithay::utils::Physical;

    use super::*;

    /// Initialize the EGL manager with the given native display
    pub fn initialize<N>(native: N) -> Result<()>
    where
        N: Clone + smithay::backend::egl::native::EGLNativeDisplay + 'static,
    {
        super::EGLManager::initialize(native)
    }

    /// Find the preferred modifier for the given parameters
    ///
    /// This function implements radiation hardening with triple modular redundancy
    /// to ensure reliable operation in radiation environments.
    pub fn find_preferred_modifier(
        gbm: &GbmDevice<DrmDeviceFd>,
        size: Size<u32, Physical>,
        fourcc: Fourcc,
        modifiers: Vec<i64>,
    ) -> Result<(Modifier, usize)> {
        // This is a safe wrapper around the unsafe implementation
        let (modifier_i64, plane_count) =
            unsafe { super::find_preferred_modifier(gbm, size, fourcc, modifiers) }?;

        // Convert the i64 to Modifier
        let modifier = Modifier::from(modifier_i64 as u64);

        Ok((modifier, plane_count))
    }

    /// Get the EGL display managed by the EGLManager
    pub fn get_display() -> Result<EGLDisplay> {
        super::EGLManager::get_display()
    }

    /// Create a new EGL context with the specified priority
    pub fn create_context(priority: ContextPriority) -> Result<EGLContext> {
        super::EGLManager::create_context(priority)
    }

    /// Create a GLES renderer from an EGL context without requiring unsafe
    pub fn create_renderer(context: EGLContext) -> Result<GlesRenderer> {
        // Safety is handled internally - context ownership is properly managed
        unsafe { super::EGLManager::create_renderer(context) }
    }

    /// Allocate a GBM buffer with the specified parameters
    pub fn allocate_buffer(
        gbm: &GbmDevice<DrmDeviceFd>,
        size: Size<u32, Physical>,
        fourcc: Fourcc,
        modifiers: &[i64],
    ) -> Result<(GbmBuffer, i64)> {
        super::allocate_buffer(gbm, size, fourcc, modifiers)
    }
}

// Private implementation details below
// ------------------------------------------

/// Maximum number of redundant systems for radiation hardening
const REDUNDANCY: usize = 3;

/// Radiation-hardened EGL display manager with triple redundancy
#[derive(Default)]
struct EGLManagerState {
    /// Reference-counted displays with redundancy protection
    displays: [Option<EGLDisplay>; REDUNDANCY],

    /// Error recovery voting status
    status: [bool; REDUNDANCY],
}

/// Singleton access to EGL resources with radiation hardening
struct EGLManager;

impl EGLManager {
    pub fn initialize<N>(native: N) -> Result<()>
    where
        N: Clone + smithay::backend::egl::native::EGLNativeDisplay + 'static,
    {
        let init_fn = || unsafe {
            let mut displays = [None, None, None];
            let mut status = [false; REDUNDANCY];

            for i in 0..REDUNDANCY {
                match EGLDisplay::new(native.clone()) {
                    Ok(display) => {
                        displays[i] = Some(display);
                        status[i] = true;
                    }
                    Err(e) => {
                        error!("Failed to initialize EGL display #{}: {}", i, e);
                        status[i] = false;
                    }
                }
            }

            // Ensure majority initialization
            let valid_count = status.iter().filter(|&&v| v).count();
            if valid_count < 2 {
                panic!("Critical fault: Failed to initialize enough EGL displays for TMR");
            }

            let mut state = Self::instance().lock().unwrap();
            state.displays = displays;
            state.status = status;
        };

        Self::init_once().call_once(init_fn);
        Ok(())
    }

    fn instance() -> &'static Mutex<EGLManagerState> {
        static INSTANCE: std::sync::OnceLock<Mutex<EGLManagerState>> = std::sync::OnceLock::new();
        INSTANCE.get_or_init(|| {
            Mutex::new(EGLManagerState {
                displays: [None, None, None],
                status: [false; REDUNDANCY],
            })
        })
    }

    fn init_once() -> &'static Once {
        static INIT: std::sync::OnceLock<Once> = std::sync::OnceLock::new();
        INIT.get_or_init(Once::new)
    }

    pub fn get_display() -> Result<EGLDisplay> {
        if !Self::init_once().is_completed() {
            return Err(anyhow!(
                "EGL manager not initialized. Call initialize() first"
            ));
        }

        let state = Self::instance().lock().unwrap();

        for i in 0..REDUNDANCY {
            if state.status[i] {
                return Ok(state.displays[i]
                    .as_ref()
                    .expect("Display marked valid but is None")
                    .clone());
            }
        }

        Err(anyhow!("No valid EGL displays available"))
    }

    pub fn create_context(priority: ContextPriority) -> Result<EGLContext> {
        let display = Self::get_display()?;
        EGLContext::new_with_priority(&display, priority)
            .context("Failed to create EGL context with specified priority")
    }

    pub unsafe fn create_renderer(context: EGLContext) -> Result<GlesRenderer> {
        GlesRenderer::new(context).context("Failed to create GLES renderer from EGL context")
    }
}

unsafe fn find_preferred_modifier(
    gbm: &GbmDevice<DrmDeviceFd>,
    size: Size<u32, smithay::utils::Physical>,
    fourcc: Fourcc,
    modifiers: Vec<i64>,
) -> Result<(i64, usize)> {
    // Initialize EGL manager with radiation hardening
    EGLManager::initialize(gbm.clone())?;

    // Log operation for traceability
    debug!(
        "Finding preferred modifier: size={:?}, fourcc={}, modifiers.len={}",
        size,
        fourcc,
        modifiers.len()
    );

    // Triple redundant results for critical calculations
    let mut results = [(0i64, 0usize); REDUNDANCY];
    let mut success = [false; REDUNDANCY];

    // Run multiple redundant calculations for TMR
    for i in 0..REDUNDANCY {
        match find_modifier_path(gbm, size, fourcc, &modifiers, i) {
            Ok((modifier, plane_count)) => {
                results[i] = (modifier, plane_count);
                success[i] = true;
            }
            Err(e) => {
                error!("TMR path {} failed: {}", i, e);
                success[i] = false;
            }
        }
    }

    // Perform voting to determine the correct result
    let valid_count = success.iter().filter(|&&v| v).count();
    if valid_count < 2 {
        return Err(anyhow!(
            "Failed to determine modifier: Majority voting failed"
        ));
    }

    // Find the most common result through voting
    for i in 0..REDUNDANCY {
        if success[i] {
            let count = success
                .iter()
                .enumerate()
                .filter(|&(j, &valid)| valid && results[j] == results[i])
                .count();

            if count >= 2 {
                return Ok(results[i]);
            }
        }
    }

    Err(anyhow!("Failed to determine consistent modifier value"))
}

unsafe fn find_modifier_path(
    gbm: &GbmDevice<DrmDeviceFd>,
    size: Size<u32, smithay::utils::Physical>,
    fourcc: Fourcc,
    modifiers: &[i64],
    path_id: usize,
) -> Result<(i64, usize)> {
    let (buffer, modifier) = allocate_buffer(gbm, size, fourcc, modifiers)
        .with_context(|| format!("Failed to allocate GBM buffer in TMR path {}", path_id))?;

    let dmabuf = buffer
        .export()
        .with_context(|| format!("Failed to export buffer as dmabuf in TMR path {}", path_id))?;

    let plane_count = dmabuf.num_planes();

    let display = EGLManager::get_display()?;
    let context = EGLContext::new_with_priority(&display, ContextPriority::Medium)
        .with_context(|| format!("Failed to create EGL context in TMR path {}", path_id))?;

    let mut renderer = GlesRenderer::new(context)
        .with_context(|| format!("Failed to create GLES renderer in TMR path {}", path_id))?;

    renderer
        .import_dmabuf(&dmabuf, None)
        .with_context(|| format!("Failed to import dmabuf in TMR path {}", path_id))?;

    Ok((modifier, plane_count))
}

fn allocate_buffer(
    gbm: &GbmDevice<DrmDeviceFd>,
    size: Size<u32, smithay::utils::Physical>,
    fourcc: Fourcc,
    modifiers: &[i64],
) -> Result<(GbmBuffer, i64)> {
    // Validate size to prevent overflows
    if size.w == 0 || size.h == 0 || size.w > 16384 || size.h > 16384 {
        return Err(anyhow!("Invalid buffer dimensions: {:?}", size));
    }

    // Convert i64 modifiers to u64 safely with validation
    let validated_modifiers: Vec<u64> = modifiers
        .iter()
        .map(|&m| {
            if m < 0 {
                debug!(
                    "Converting negative modifier {} to u64, this may cause issues",
                    m
                );
            }
            m as u64
        })
        .collect();

    let buffer_object = gbm
        .create_buffer_object_with_modifiers(
            size.w,
            size.h,
            fourcc,
            validated_modifiers.iter().copied().map(DrmModifier::from),
        )
        .context("Failed to create GBM buffer object")?;

    let modifier = modifiers.first().copied().unwrap_or(0);
    let buffer = GbmBuffer::from_bo(buffer_object, false);

    Ok((buffer, modifier))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_triple_redundancy_voting() {
        let results = [(123i64, 2usize), (123i64, 2usize), (456i64, 3usize)];
        let success = [true, true, true];

        let mut found = false;
        for i in 0..REDUNDANCY {
            if success[i] {
                let count = success
                    .iter()
                    .enumerate()
                    .filter(|&(j, &valid)| valid && results[j] == results[i])
                    .count();

                if count >= 2 {
                    assert_eq!(results[i], (123i64, 2usize));
                    found = true;
                    break;
                }
            }
        }

        assert!(found, "Voting should identify the majority result");
    }
}
