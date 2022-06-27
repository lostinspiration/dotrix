//! Dotrix is a 3D game engine following ECS programming pattern with a goal to be simple and
//! feature rich. There is a [Löwenware](https://lowenware.com) team behind the project and we
//! are working on Dotrix to power up our own game projects.
//!
//! The best place to see what can be done with Dotrix is our
//! [YouTube](https://www.youtube.com/channel/UCdriNXRizbBFQhqZefaw44A) channel.
//!
//! ## Getting Started
//!
//! If you are more into a practice and looking for an example code, we've prepared a good
//! [demo application](https://github.com/lowenware/dotrix/blob/main/examples/demo/demo.rs) for you
//! to get started.
//!
//! If you prefer to work with the documentation, then the best place to start is where your
//! game should start - the [`Dotrix`] application builder.
//!
//! ```no_run
//! use dotrix::Dotrix;
//!
//! fn main() {
//!     Dotrix::application("My Game")
//!         .run();
//! }
//! ```
//!
//! It is also a tool to say the engine where should be rendered the output and what services
//! and systems has to be enabled.

#![doc(html_logo_url = "https://raw.githubusercontent.com/lowenware/dotrix/master/logo.png")]
#![warn(missing_docs)]

pub use dotrix_os::*;

/*
pub use dotrix_core::*;
pub use dotrix_math as math;

#[cfg(feature = "egui")]
pub use dotrix_egui as egui;

#[cfg(feature = "overlay")]
pub use dotrix_overlay as overlay;

#[cfg(feature = "pbr")]
pub use dotrix_pbr as pbr;

#[cfg(feature = "primitives")]
pub use dotrix_primitives as primitives;

#[cfg(feature = "sky")]
pub use dotrix_sky as sky;

#[cfg(feature = "terrain")]
pub use dotrix_terrain as terrain;

pub mod prelude {
    pub use crate::Dotrix;
    pub use dotrix_core::ecs::{Const, Context, Mut, System};
    pub use dotrix_core::Service;
    pub use dotrix_core::{Color, Id};
}
*/

/// Application Builder
///
/// This structure is supposed to be constructed only once and usually inside of a main
/// function
///
/// You can also check full functional
/// [Dotrix Demo](https://github.com/lowenware/dotrix/blob/main/examples/demo/main.rs) example to
/// learn more about the builder.
pub struct Dotrix {
    name: String,
}

impl Dotrix {
    /// Initiates building of an application with specified name
    pub fn application(name: &'static str) -> Self {
        Self::bare(name)
    }

    /// Initiates building of an application with specified name
    pub fn bare(name: &'static str) -> Self {
        Self {
            name: String::from(name),
        }
    }

    /*
    #[must_use]
    /// Adds service to the application
    pub fn with<T>(mut self, service: T) -> Self
    where
        T: Service,
    {
        println!("Run Level is {}", service.run_level());
        // if let Some(startup) = Service::get_startup::<T>() {
        //    self.startup.push(startup);
        //}
        // if let Some(bind) = Service::get_bind::<T>() {
        //    self.bind.push(startup);
        //}
        self.storage.insert(TypeId::of::<T>(), Box::new(service));
        self
    }
    */

    /// Runs the application
    pub fn run(self) {}
}

/*
/// Trait providing extendablity
pub trait ExtendWith<T> {
    /// Extends self using the `extension` function
    fn extend_with(&mut self, extension: T);
}

impl ExtendWith<System> for Dotrix {
    fn extend_with(&mut self, extension: System) {
        self.app.as_mut().unwrap().add_system(extension);
    }
}

impl<T: IntoService> ExtendWith<Service<T>> for Dotrix {
    fn extend_with(&mut self, extension: Service<T>) {
        self.app.as_mut().unwrap().add_service(extension.node);
    }
}

impl<T: FnOnce(&mut Application)> ExtendWith<T> for Dotrix {
    fn extend_with(&mut self, extension: T) {
        extension(self.app.as_mut().unwrap())
    }
}
*/
