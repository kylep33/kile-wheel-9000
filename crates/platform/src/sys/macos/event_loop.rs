//! The AppKit event loop, run one event at a time so the wheel keeps its own loop.
//!
//! The tray, the overlay panel and the yes/no alert all need AppKit's events dispatched
//! on the main thread. A wake from another thread posts an application-defined event,
//! which ends the wait.

use std::time::Instant;

use objc2::MainThreadMarker;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSEvent, NSEventMask, NSEventModifierFlags,
    NSEventType,
};
use objc2_foundation::{NSDate, NSDefaultRunLoopMode, NSPoint};
use thiserror::Error;

#[derive(Debug, Error)]
#[error("the event loop has to be created on the main thread, which AppKit insists on")]
pub struct LoopError;

/// Holds the shared application by address, because AppKit types cannot cross threads;
/// posting an event is one of the few calls AppKit allows from any thread.
#[derive(Debug, Clone)]
pub struct Waker {
    app: usize,
}

impl Waker {
    pub fn wake(&self) {
        let event = NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
            NSEventType::ApplicationDefined,
            NSPoint::new(0.0, 0.0),
            NSEventModifierFlags::empty(),
            0.0,
            0,
            None,
            0,
            0,
            0,
        );
        let Some(event) = event else {
            return;
        };

        // SAFETY: the address is the shared NSApplication, which lives for the process,
        // and postEvent:atStart: is documented as safe to call from other threads.
        unsafe {
            let app = &*(self.app as *const AnyObject);
            let _: () = objc2::msg_send![app, postEvent: &*event, atStart: false];
        }
    }
}

#[derive(Debug)]
pub struct Loop {
    app: Retained<NSApplication>,
}

impl Loop {
    pub fn new() -> Result<(Self, Waker), LoopError> {
        let mtm = MainThreadMarker::new().ok_or(LoopError)?;
        let app = NSApplication::sharedApplication(mtm);

        // no Dock icon and no menu bar of our own: the wheel is a tray app
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        app.finishLaunching();

        let waker = Waker {
            app: Retained::as_ptr(&app) as usize,
        };
        Ok((Self { app }, waker))
    }

    pub fn wait(&mut self, deadline: Option<Instant>) {
        autoreleasepool(|_| {
            let until = match deadline {
                Some(deadline) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    NSDate::dateWithTimeIntervalSinceNow(left.as_secs_f64())
                }
                None => NSDate::distantFuture(),
            };
            if !self.dispatch_one(&until) {
                return;
            }
            // everything already queued goes too, without waiting again
            let now = NSDate::distantPast();
            while self.dispatch_one(&now) {}
        });
    }

    /// Reports whether an event arrived before `until`.
    fn dispatch_one(&self, until: &NSDate) -> bool {
        // SAFETY: the mode constant is a static Foundation owns for the process lifetime.
        let mode = unsafe { NSDefaultRunLoopMode };
        let event = self
            .app
            .nextEventMatchingMask_untilDate_inMode_dequeue(NSEventMask::Any, Some(until), mode, true);
        let Some(event) = event else {
            return false;
        };
        if event.r#type() != NSEventType::ApplicationDefined {
            self.app.sendEvent(&event);
        }
        true
    }
}
