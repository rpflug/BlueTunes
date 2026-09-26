//! A nonblocking PCM tap and an audio-reactive, native egui canvas.
use eframe::egui::{self, Color32, Pos2, Stroke};
use rodio::Source;
use std::{
    f32::consts::TAU,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone, Default)]
pub struct Levels {
    pub energy: f32,
    pub bass: f32,
    pub wave: Vec<f32>,
}
pub type Meter = Arc<Mutex<Levels>>;
pub fn meter() -> Meter {
    Arc::new(Mutex::new(Levels {
        wave: Vec::with_capacity(512),
        ..Default::default()
    }))
}

pub struct Tap<S> {
    source: S,
    meter: Meter,
    sum: f32,
    bass_sum: f32,
    low: f32,
    alpha: f32,
    frame: usize,
    wave: Vec<f32>,
}
impl<S: Source> Tap<S> {
    pub fn new(source: S, meter: Meter) -> Self {
        let alpha = 1. - (-TAU * 180. / source.sample_rate() as f32).exp();
        Self {
            alpha,
            source,
            meter,
            sum: 0.,
            bass_sum: 0.,
            low: 0.,
            frame: 0,
            wave: Vec::with_capacity(512),
        }
    }
}
impl<S: Source> Iterator for Tap<S> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let sample = self.source.next()?;
        // Sample the first channel once per frame; never alter the audio stream.
        if self.frame.is_multiple_of(self.source.channels() as usize) {
            let x = if sample.is_finite() { sample } else { 0. };
            self.low += self.alpha * (x - self.low);
            self.sum += x * x;
            self.bass_sum += self.low * self.low;
            self.wave.push(x);
            if self.wave.len() == 512 {
                // Audio callback must never wait for the UI thread.
                if let Ok(mut meter) = self.meter.try_lock() {
                    meter.energy = (self.sum / 512.).sqrt();
                    meter.bass = (self.bass_sum / 512.).sqrt();
                    meter.wave.clear();
                    meter.wave.extend_from_slice(&self.wave);
                }
                self.wave.clear();
                self.sum = 0.;
                self.bass_sum = 0.;
            }
        }
        self.frame = self.frame.wrapping_add(1);
        Some(sample)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.source.size_hint()
    }
}
impl<S: Source> Source for Tap<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.source.current_span_len()
    }
    fn channels(&self) -> u16 {
        self.source.channels()
    }
    fn sample_rate(&self) -> u32 {
        self.source.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.source.total_duration()
    }
    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        self.source.try_seek(pos)?;
        self.wave.clear();
        self.sum = 0.;
        self.bass_sum = 0.;
        self.low = 0.;
        self.frame = 0;
        if let Ok(mut m) = self.meter.try_lock() {
            m.energy = 0.;
            m.bass = 0.;
            m.wave.clear();
        }
        Ok(())
    }
}
#[derive(Default)]
pub struct Visualizer {
    pub visible: bool,
    mode: usize,
    time: f32,
    energy: f32,
    bass: f32,
}
impl Visualizer {
    pub fn show(&mut self, ui: &mut egui::Ui, meter: &Meter, playing: bool, title: &str) {
        ui.horizontal(|ui| {
            ui.heading("Visualizer");
            ui.add_space(15.);
            for (i, name) in ["Aurora", "Kaleidoscope", "Waveform"].iter().enumerate() {
                ui.selectable_value(&mut self.mode, i, *name);
            }
            if ui.button("Back to music").clicked() {
                self.visible = false;
            }
        });
        ui.label(
            egui::RichText::new(if title.is_empty() {
                "Play a song to bring the patterns to life"
            } else {
                title
            })
            .color(Color32::from_rgb(103, 232, 227)),
        );
        let levels = meter.lock().map(|m| m.clone()).unwrap_or_default();
        let dt = ui.input(|i| i.stable_dt).min(0.05);
        let mix = 1. - (-dt * 10.).exp();
        self.energy += (if playing {
            (levels.energy * 3.).min(1.)
        } else {
            0.
        } - self.energy)
            * mix;
        self.bass += (if playing {
            (levels.bass * 5.).min(1.)
        } else {
            0.
        } - self.bass)
            * mix;
        if playing {
            self.time += dt * (0.35 + self.energy);
        }
        if playing || self.energy > 0.001 {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
        let (rect, _) = ui.allocate_exact_size(
            ui.available_size().max(egui::vec2(1., 1.)),
            egui::Sense::hover(),
        );
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 8., Color32::from_rgb(4, 8, 17));
        let c = rect.center();
        let radius = rect.width().min(rect.height()) * 0.33;
        let t = self.time;
        let color = |n: f32, alpha: u8| {
            let hue = (t * 0.025 + n).fract();
            let rgb = egui::ecolor::Hsva::new(hue, 0.65, 0.95, 1.);
            let col = Color32::from(rgb);
            Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), alpha)
        };
        match self.mode {
            0 => {
                for layer in 0..9 {
                    let n = layer as f32;
                    let points: Vec<_> = (0..200)
                        .map(|i| {
                            let u = i as f32 / 199.;
                            let x = rect.left() + u * rect.width();
                            let y = c.y
                                + (u * TAU * 2. + t + n * 0.27).sin()
                                    * radius
                                    * (0.2 + self.energy * 0.7)
                                + (u * TAU * 3. - t * 0.7 + n * 0.5).cos() * radius * 0.22
                                + (n - 4.) * 9.;
                            Pos2::new(x, y)
                        })
                        .collect();
                    p.add(egui::Shape::line(
                        points.clone(),
                        Stroke::new(12.0_f32, color(n * 0.045, 12)),
                    ));
                    p.add(egui::Shape::line(
                        points,
                        Stroke::new(1.5 + self.bass * 3., color(n * 0.045, 180)),
                    ));
                }
            }
            1 => {
                for layer in 0..7 {
                    let n = layer as f32;
                    let points: Vec<_> = (0..=360)
                        .map(|i| {
                            let a = i as f32 / 360. * TAU;
                            let r = radius
                                * (0.3 + n * 0.095)
                                * (1. + (a * 6. + t + n * 0.4).sin() * (0.15 + self.bass * 0.3));
                            let angle = a + t * 0.15;
                            Pos2::new(c.x + angle.cos() * r, c.y + angle.sin() * r)
                        })
                        .collect();
                    p.add(egui::Shape::line(
                        points.clone(),
                        Stroke::new(10.0_f32, color(n * 0.065, 15)),
                    ));
                    p.add(egui::Shape::line(
                        points,
                        Stroke::new(1.5 + self.energy * 2., color(n * 0.065, 200)),
                    ));
                }
            }
            _ => {
                for layer in 0..3 {
                    let n = layer as f32;
                    let points: Vec<_> = (0..512)
                        .map(|i| {
                            let x = rect.left() + rect.width() * i as f32 / 511.;
                            let v = if playing {
                                levels.wave.get(i).copied().unwrap_or(0.)
                            } else {
                                0.
                            };
                            Pos2::new(
                                x,
                                c.y + (n - 1.) * radius * 0.5 + v.clamp(-1., 1.) * radius * 0.9,
                            )
                        })
                        .collect();
                    p.add(egui::Shape::line(
                        points,
                        Stroke::new(2.0_f32, color(n * 0.12, 210)),
                    ));
                }
            }
        }
        // Orbiting particles brighten and spread with the measured audio energy.
        for i in 0..45 {
            let n = i as f32;
            let a = n * 2.39996 + t * (0.08 + (i % 4) as f32 * 0.03);
            let r = radius * (0.3 + (n / 45.) * 1.15 + self.bass * 0.15);
            let pos = c + egui::vec2(a.cos() * r, a.sin() * r);
            p.circle_filled(pos, 1. + self.energy * 2., color(n * 0.019, 110));
        }
        p.text(
            rect.left_bottom() + egui::vec2(14., -14.),
            egui::Align2::LEFT_BOTTOM,
            if playing {
                "SPACE  pause     ESC  back to music"
            } else {
                "Paused · press Play to animate     ESC  back to music"
            },
            egui::FontId::proportional(12.),
            Color32::from_rgb(145, 164, 188),
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_modes_render_and_pause_freezes_time() {
        let ctx = egui::Context::default();
        let m = meter();
        *m.lock().unwrap() = Levels {
            energy: 0.3,
            bass: 0.2,
            wave: vec![0.1; 512],
        };
        let mut v = Visualizer::default();
        for mode in 0..3 {
            v.mode = mode;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        Pos2::ZERO,
                        egui::vec2(1000., 600.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| v.show(ui, &m, true, "Test song"));
                },
            );
            assert!(!output.shapes.is_empty());
            let before = v.time;
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| v.show(ui, &m, false, "Paused"));
            });
            assert_eq!(v.time, before);
        }
    }
    #[test]
    fn tap_preserves_samples_and_detects_energy() {
        let data: Vec<f32> = (0..4096)
            .map(|i| (i as f32 * TAU * 80. / 48000.).sin() * 0.4)
            .collect();
        let m = meter();
        let source = rodio::buffer::SamplesBuffer::new(1, 48000, data.clone());
        let mut tap = Tap::new(source, m.clone());
        assert_eq!(tap.channels(), 1);
        assert_eq!(tap.sample_rate(), 48000);
        assert_eq!(tap.by_ref().collect::<Vec<_>>(), data);
        let levels = m.lock().unwrap();
        assert!(levels.energy > 0.2);
        assert!(levels.bass > 0.15);
        assert_eq!(levels.wave.len(), 512);
    }
    #[test]
    fn silence_and_seek_clear_meter() {
        let m = meter();
        let mut tap = Tap::new(
            rodio::buffer::SamplesBuffer::new(2, 48000, vec![0.; 8192]),
            m.clone(),
        );
        tap.by_ref().take(2048).for_each(drop);
        assert_eq!(m.lock().unwrap().energy, 0.);
        tap.try_seek(Duration::ZERO).unwrap();
        assert!(m.lock().unwrap().wave.is_empty());
    }
    #[test]
    fn busy_meter_never_blocks_audio() {
        let m = meter();
        let tap = Tap::new(
            rodio::buffer::SamplesBuffer::new(1, 48000, vec![0.5; 4096]),
            m.clone(),
        );
        let _guard = m.lock().unwrap();
        assert_eq!(tap.count(), 4096);
    }
}
