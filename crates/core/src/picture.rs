//! The large image view's geometry: how far an image is zoomed and where it
//! sits in its room (the window below the viewer's bar), in points. Zoom 1
//! is the image's own pixels (1:1); a pan moves the image's centre off the
//! room's.

/// The closest zoom: details of a screenshot at 8 screen pixels each.
pub const MAX_ZOOM: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Picture {
    /// The image's own size and its room's, in points.
    pub image: (f32, f32),
    pub window: (f32, f32),
    pub zoom: f32,
    pub pan: (f32, f32),
    /// Fitted, at most this zoom: a resized room fits it again. None once
    /// zoomed by hand.
    pub fitting: Option<f32>,
}

impl Picture {
    /// As it opens: fitted, never past its own size.
    pub fn new(image: (f32, f32), window: (f32, f32)) -> Self {
        Self {
            image: (image.0.max(1.0), image.1.max(1.0)),
            window,
            zoom: 1.0,
            pan: (0.0, 0.0),
            fitting: None,
        }
        .fitted(1.0)
    }

    /// The zoom that shows all of it, edge to edge: up from a small one.
    pub fn fit(&self) -> f32 {
        let room = |w: f32, i: f32| w.max(1.0) / i;
        room(self.window.0, self.image.0).min(room(self.window.1, self.image.1))
    }

    /// Fitted, at most at `cap` (1 as it opens, [`MAX_ZOOM`] for Fit), and
    /// fitted again whenever the room is resized.
    pub fn fitted(self, cap: f32) -> Self {
        Self {
            fitting: Some(cap),
            ..self.zoomed(self.fit().min(cap))
        }
    }

    /// The farthest zoom: a quarter of the fit, or of 1:1 if that is less.
    fn min_zoom(&self) -> f32 {
        self.fit().min(1.0) / 4.0
    }

    /// The image's size as shown.
    pub fn shown(&self) -> (f32, f32) {
        (self.image.0 * self.zoom, self.image.1 * self.zoom)
    }

    /// The image's top left in the room.
    pub fn origin(&self) -> (f32, f32) {
        let (w, h) = self.shown();
        (
            (self.window.0 - w) / 2.0 + self.pan.0,
            (self.window.1 - h) / 2.0 + self.pan.1,
        )
    }

    /// At `zoom`, centred.
    pub fn zoomed(self, zoom: f32) -> Self {
        Self {
            zoom: zoom.clamp(self.min_zoom(), MAX_ZOOM),
            pan: (0.0, 0.0),
            fitting: None,
            ..self
        }
    }

    /// At `zoom`, the point under `at` (in the room) kept under it.
    pub fn zoomed_at(self, zoom: f32, at: (f32, f32)) -> Self {
        let zoom = zoom.clamp(self.min_zoom(), MAX_ZOOM);
        let (x, y) = self.origin();
        let (u, v) = ((at.0 - x) / self.zoom, (at.1 - y) / self.zoom);
        let next = Self {
            zoom,
            fitting: None,
            ..self
        };
        let (w, h) = next.shown();
        let pan = (
            at.0 - u * zoom - (self.window.0 - w) / 2.0,
            at.1 - v * zoom - (self.window.1 - h) / 2.0,
        );
        Self { pan, ..next }.held()
    }

    /// Moved by `by` points, as a drag moves it. A fitted image has nowhere
    /// to go and stays fitted.
    pub fn panned(self, from: (f32, f32), by: (f32, f32)) -> Self {
        Self {
            pan: (from.0 + by.0, from.1 + by.1),
            ..self
        }
        .held()
    }

    /// In a room resized to `window`: fitted again if it was, else the pan
    /// held to its edges.
    pub fn in_window(self, window: (f32, f32)) -> Self {
        let resized = Self { window, ..self };
        match self.fitting {
            Some(cap) => resized.fitted(cap),
            None => resized.held(),
        }
    }

    /// The pan kept so an image larger than the room still covers it, and
    /// a smaller one stays centred.
    fn held(self) -> Self {
        let (w, h) = self.shown();
        let hold = |pan: f32, shown: f32, window: f32| {
            let slack = ((shown - window) / 2.0).max(0.0);
            pan.clamp(-slack, slack)
        };
        Self {
            pan: (
                hold(self.pan.0, w, self.window.0),
                hold(self.pan.1, h, self.window.1),
            ),
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: (f32, f32) = (1000.0, 800.0);

    #[test]
    fn it_opens_fitted_edge_to_edge_never_past_its_own_size() {
        let big = Picture::new((2000.0, 1000.0), WINDOW);
        assert_eq!(big.shown(), (1000.0, 500.0), "no margin");
        assert_eq!(big.origin(), (0.0, 150.0));
        let small = Picture::new((300.0, 200.0), WINDOW);
        assert_eq!(small.shown(), (300.0, 200.0), "1:1, not blown up");
        let (w, h) = small.fitted(MAX_ZOOM).shown();
        assert_eq!((w, h.round()), (1000.0, 667.0), "Fit blows it up");
    }

    #[test]
    fn a_fitted_image_follows_the_window_until_zoomed_by_hand() {
        let p = Picture::new((2000.0, 1000.0), WINDOW);
        let wider = p.in_window((1600.0, 1000.0));
        assert_eq!(wider.shown(), (1600.0, 800.0), "fitted again");
        let small = Picture::new((300.0, 200.0), WINDOW);
        assert_eq!(small.in_window((2000.0, 2000.0)).zoom, 1.0, "still 1:1");
        let blown = small.fitted(MAX_ZOOM).in_window((600.0, 2000.0));
        assert_eq!(blown.shown(), (600.0, 400.0), "Fit's, not the opening's");
        let dragged = p.panned(p.pan, (50.0, 50.0));
        assert_eq!(dragged.in_window((1600.0, 1000.0)), wider, "still fitted");
        let zoomed = p.zoomed(1.0).in_window((1600.0, 1000.0));
        assert_eq!(zoomed.zoom, 1.0, "a hand zoom stays");
        let wheeled = p.zoomed_at(0.6, (10.0, 10.0));
        assert_eq!(wheeled.in_window((1600.0, 1000.0)).zoom, 0.6);
    }

    #[test]
    fn a_zoom_keeps_the_point_under_the_pointer() {
        let p = Picture::new((2000.0, 1000.0), WINDOW);
        let at = (300.0, 300.0);
        let (x, y) = p.origin();
        let under = ((at.0 - x) / p.zoom, (at.1 - y) / p.zoom);
        let z = p.zoomed_at(2.0, at);
        let (x, y) = z.origin();
        assert!((x + under.0 * 2.0 - at.0).abs() < 0.01);
        assert!((y + under.1 * 2.0 - at.1).abs() < 0.01);
        assert_eq!(p.zoomed_at(100.0, at).zoom, MAX_ZOOM);
        assert_eq!(p.zoomed_at(0.0, at).zoom, p.zoom / 4.0);
    }

    #[test]
    fn a_pan_stops_at_the_edges_and_a_small_image_stays_centred() {
        let p = Picture::new((2000.0, 1000.0), WINDOW).zoomed(1.0);
        let far = p.panned(p.pan, (5000.0, -5000.0));
        assert_eq!(
            far.origin(),
            (0.0, WINDOW.1 - 1000.0),
            "its corner at the window's"
        );
        let small = Picture::new((300.0, 200.0), WINDOW);
        assert_eq!(small.panned(small.pan, (100.0, 100.0)).pan, (0.0, 0.0));
    }
}
