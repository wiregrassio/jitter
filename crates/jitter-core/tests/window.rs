//! Window and counter contracts.
#![allow(clippy::unwrap_used, missing_docs, non_snake_case)]

use jitter_core::counter::{Backwards, Last, delta};
use jitter_core::stats::{Window, WindowError};

#[test]
fn window__empty_closes_to_none_not_zeros() {
    let mut w = Window::with_capacity(4);
    assert_eq!(w.close(), None);
}

#[test]
fn window__refuses_past_capacity_and_garbage() {
    let mut w = Window::with_capacity(2);
    w.push(1.0).unwrap();
    w.push(2.0).unwrap();
    assert_eq!(w.push(3.0), Err(WindowError::Full { capacity: 2 }));
    assert_eq!(w.push(f64::NAN), Err(WindowError::NotFinite));
}

#[test]
fn window__close_resets_for_the_next_window() {
    let mut w = Window::with_capacity(3);
    for v in [5.0, 1.0, 3.0] {
        w.push(v).unwrap();
    }
    let s = w.close().unwrap();
    assert_eq!(
        (s.count, s.min, s.p50, s.max, s.mean),
        (3, 1.0, 3.0, 5.0, 3.0)
    );
    assert!(w.is_empty());
    w.push(9.0).unwrap();
    assert_eq!(w.close().unwrap().max, 9.0);
}

#[test]
fn counter__first_read_has_no_rate() {
    let mut last = Last::default();
    assert_eq!(last.advance(100).unwrap(), None);
    assert_eq!(last.advance(150).unwrap(), Some(50));
}

#[test]
fn counter__backwards_is_an_error() {
    assert_eq!(
        delta(10, 9),
        Err(Backwards {
            previous: 10,
            current: 9
        })
    );
}
