//! wgpu renderer: GPU-generated CDLOD terrain, cascaded shadows with static caching, instanced
//! objects with GPU culling (frustum, Hi-Z, LOD) and indirect multi-draws, sky, post, overlay —
//! each a render system registered in the frame graph. It knows bodies, models and instances,
//! never what a ship or a moon is.
pub mod capture;
pub mod context;
mod detail;
pub mod frame;
mod globals;
mod governor;
pub mod graph;
pub mod overlay;
pub mod particles;
pub mod plumes;
pub mod prints;
mod pipes;
mod post;
pub mod bodies;
pub mod props;
pub mod renderer;
pub mod scene;
mod shadow;
mod sky;
pub mod structures;
mod targets;
pub mod terrain;
pub mod timing;
mod uniforms;

pub use frame::View;
pub use wgpu;
pub use overlay::UiFrame;
pub use context::Screen;
pub use renderer::{FrameInput, Renderer, Stats};
pub use uniforms::Light;
pub use post::Visor;

/// A shader module from WGSL parts concatenated in order (shared structs first).
pub(crate) fn shader(device: &wgpu::Device, label: &str, parts: &[&str]) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(label), source: wgpu::ShaderSource::Wgsl(parts.concat().into()) })
}

/// Initialization only: a tiny executor for wgpu's adapter/device futures.
pub fn wait_for<T>(future: impl std::future::Future<Output = T>) -> T {
    use std::{
        sync::Arc,
        task::{Context, Poll, Wake, Waker},
    };
    struct ThreadWake(std::thread::Thread);
    impl Wake for ThreadWake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::park(),
        }
    }
}
