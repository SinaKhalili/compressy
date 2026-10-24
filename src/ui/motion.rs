//! Motion, as Zeron's (`zeron_theme.rs` in zgui's examples): CSS curves,
//! duration specs, reversible tweens, and a page clock that only runs
//! while something moves, so an idle window does no work.
use std::{cell::Cell, rc::Rc, time::Instant};
use zgui::{
    compose::{Tasks, prelude::*},
    frame::FrameClock,
    reactive::Signal,
};

/// A CSS cubic-bezier: Newton steps, then bisection.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Curve(pub f32, pub f32, pub f32, pub f32);

impl Curve {
    pub fn eval(self, x: f32) -> f32 {
        let x = x.clamp(0., 1.);
        if x <= 0. || x >= 1. {
            return x;
        }
        let Curve(x1, y1, x2, y2) = self;
        let bez = |t: f32, a: f32, b: f32| {
            let u = 1. - t;
            3. * u * u * t * a + 3. * u * t * t * b + t * t * t
        };
        let slope = |t: f32, a: f32, b: f32| {
            let u = 1. - t;
            3. * u * u * a + 6. * u * t * (b - a) + 3. * t * t * (1. - b)
        };
        let mut t = x;
        for _ in 0..8 {
            let error = bez(t, x1, x2) - x;
            if error.abs() < 1e-6 {
                return bez(t, y1, y2).clamp(0., 1.);
            }
            let d = slope(t, x1, x2);
            if d.abs() < 1e-6 {
                break;
            }
            t -= error / d;
        }
        let (mut lo, mut hi) = (0_f32, 1_f32);
        t = x;
        for _ in 0..32 {
            let value = bez(t, x1, x2);
            if (value - x).abs() < 1e-6 {
                break;
            }
            if value < x {
                lo = t;
            } else {
                hi = t;
            }
            t = (lo + hi) / 2.;
        }
        bez(t, y1, y2).clamp(0., 1.)
    }
}

pub const EASE_OUT_EXPO: Curve = Curve(0.16, 1., 0.3, 1.);
pub const EASE_TAILWIND: Curve = Curve(0.4, 0., 0.2, 1.);

/// A duration (seconds) and its curve.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Spec(pub f32, pub Curve);

impl Spec {
    pub fn at(self, elapsed: f32) -> f32 {
        self.1.eval(elapsed / self.0)
    }
}

pub const FADE_IN: Spec = Spec(0.5, EASE_OUT_EXPO);
pub const HOVER_FADE: Spec = Spec(0.15, EASE_TAILWIND);

/// A value easing from `from` to `target`, starting at `at`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Tween {
    pub from: f32,
    pub target: f32,
    pub at: f32,
}

impl Tween {
    pub fn settled(value: f32) -> Self {
        Self {
            from: value,
            target: value,
            at: f32::NEG_INFINITY,
        }
    }
    pub fn value(self, now: f32, spec: Spec) -> f32 {
        self.from + (self.target - self.from) * spec.at(now - self.at)
    }
    /// Retarget from wherever it is now, so reversals don't jump.
    pub fn toward(self, target: f32, now: f32, spec: Spec) -> Self {
        Self {
            from: self.value(now, spec),
            target,
            at: now,
        }
    }
    pub fn end(self, spec: Spec) -> f32 {
        self.at + spec.0
    }
}

/// Looping decoration (the working pulse) runs at Zeron's pulse rate.
const AMBIENT_HZ: f64 = 30.;

/// The page clock: full rate until `until`, `AMBIENT_HZ` while an ambient
/// loop is registered, stopped otherwise.
#[derive(Clone)]
pub struct Anim {
    pub clock: Signal<f32>,
    pub ambient_clock: Signal<f32>,
    origin: Instant,
    until: Rc<Cell<f32>>,
    ambient: Rc<Cell<u32>>,
    running: Rc<Cell<bool>>,
    tasks: Tasks,
    frames: FrameClock,
}

impl Anim {
    pub fn new(cx: &mut Context) -> Self {
        Self {
            clock: cx.state(0.),
            ambient_clock: cx.state(0.),
            origin: Instant::now(),
            until: Rc::new(Cell::new(0.)),
            ambient: Rc::new(Cell::new(0)),
            running: Rc::new(Cell::new(false)),
            tasks: cx.tasks(),
            frames: cx.frames(),
        }
    }
    pub fn now(&self) -> f32 {
        self.origin.elapsed().as_secs_f32()
    }
    /// Keep full-rate frames coming for `seconds` more.
    pub fn keep(&self, seconds: f32) {
        self.until.set(self.until.get().max(self.now() + seconds));
        self.run();
    }
    /// Register an ambient loop for as long as the guard lives.
    pub fn ambient(&self) -> AmbientGuard {
        self.ambient.set(self.ambient.get() + 1);
        self.run();
        AmbientGuard(self.ambient.clone())
    }
    fn run(&self) {
        if self.running.replace(true) {
            return;
        }
        let anim = self.clone();
        self.tasks.spawn(async move {
            loop {
                let full = anim.now() < anim.until.get();
                if !full && anim.ambient.get() == 0 {
                    break;
                }
                let frame = if full {
                    anim.frames.next().await
                } else {
                    anim.frames.next().max_rate(AMBIENT_HZ).await
                };
                let at = frame
                    .time
                    .saturating_duration_since(anim.origin)
                    .as_secs_f32();
                let at = at.max(anim.clock.with_untracked(|c| *c));
                anim.clock.set(at);
                let last = anim.ambient_clock.with_untracked(|c| *c);
                if at - last >= 1. / AMBIENT_HZ as f32 - 0.001 {
                    anim.ambient_clock.set(at);
                }
            }
            anim.running.set(false);
        });
    }
    /// The time, tracked until `end` and constant afterwards.
    pub fn at(&self, end: f32) -> f32 {
        let now = self.clock.with_untracked(|now| *now);
        if now >= end { now } else { self.clock.get() }
    }
    /// A tween's value, tracking the clock only while it moves.
    pub fn tween(&self, tween: &Signal<Tween>, spec: Spec) -> f32 {
        let t = tween.get();
        t.value(self.at(t.end(spec)), spec)
    }
    /// Retarget a tween and keep frames coming while it moves.
    pub fn retarget(&self, tween: &Signal<Tween>, target: f32, spec: Spec) {
        let now = self.now();
        if tween.with_untracked(|t| t.target == target) {
            return;
        }
        tween.update(|t| *t = t.toward(target, now, spec));
        self.keep(spec.0 + 0.05);
    }
    /// 0 to 1 over `FADE_IN` from now: a screen arriving.
    pub fn entrance(&self) -> impl Fn() -> f32 + Clone + 'static {
        let anim = self.clone();
        let born = anim.now();
        anim.keep(FADE_IN.0 + 0.05);
        move || FADE_IN.at(anim.at(born + FADE_IN.0) - born)
    }
}

pub struct AmbientGuard(Rc<Cell<u32>>);

impl Drop for AmbientGuard {
    fn drop(&mut self) {
        self.0.set(self.0.get().saturating_sub(1));
    }
}
