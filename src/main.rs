mod artwork;
mod browser;
mod settings;
mod visualizer;
use eframe::egui::{self, Color32, RichText};
use egui_extras::{Column, TableBuilder};
use lofty::{
    file::{AudioFile, TaggedFileExt},
    tag::Accessor,
};
use rodio::{Decoder, OutputStream, OutputStreamBuilder, Sink};
use serde::{Deserialize, Serialize};
use settings::Settings;
use std::{
    collections::{BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};

const BG: Color32 = Color32::from_rgb(7, 11, 18);
const PANEL: Color32 = Color32::from_rgb(13, 20, 32);
const BLUE: Color32 = Color32::from_rgb(98, 184, 255);
const CYAN: Color32 = Color32::from_rgb(103, 232, 227);
const MUTED: Color32 = Color32::from_rgb(145, 164, 188);

#[derive(Clone, Serialize, Deserialize, Debug)]
struct Track {
    path: PathBuf,
    title: String,
    artist: String,
    album: String,
    genre: String,
    number: u32,
    seconds: u64,
    format: String,
}
#[derive(Default, Serialize, Deserialize)]
struct Playlist {
    name: String,
    paths: Vec<PathBuf>,
}
#[derive(Clone)]
struct PlaylistDrag {
    playlist: usize,
    path: PathBuf,
}
impl Playlist {
    fn move_relative(&mut self, source: &Path, target: &Path, after: bool) -> bool {
        if source == target {
            return false;
        }
        let Some(from) = self.paths.iter().position(|p| p == source) else {
            return false;
        };
        let Some(to) = self.paths.iter().position(|p| p == target) else {
            return false;
        };
        let insertion = to + usize::from(after);
        let insertion = if from < insertion {
            insertion - 1
        } else {
            insertion
        };
        if insertion == from {
            return false;
        }
        let path = self.paths.remove(from);
        self.paths.insert(insertion, path);
        true
    }
    fn order_ids(&self, tracks: &[Track], ids: &mut [usize]) {
        let positions: HashMap<_, _> = self.paths.iter().enumerate().map(|(i, p)| (p, i)).collect();
        ids.sort_by_key(|id| {
            positions
                .get(&tracks[*id].path)
                .copied()
                .unwrap_or(usize::MAX)
        });
    }

    fn add_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>) -> usize {
        let mut added = 0;
        for path in paths {
            if !self.paths.contains(&path) {
                self.paths.push(path);
                added += 1;
            }
        }
        added
    }
}
#[derive(Default, Serialize, Deserialize)]
struct Library {
    tracks: Vec<Track>,
    playlists: Vec<Playlist>,
}
fn supported(path: &Path) -> bool {
    path.extension()
        .and_then(|x| x.to_str())
        .is_some_and(|x| x.eq_ignore_ascii_case("mp3") || x.eq_ignore_ascii_case("flac"))
}
fn read_track(path: &Path) -> Result<Track, String> {
    let f = lofty::read_from_path(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let tag = f.primary_tag().or_else(|| f.first_tag());
    let text = |v: Option<std::borrow::Cow<'_, str>>, fallback: &str| {
        v.filter(|v| !v.trim().is_empty())
            .map(|v| v.into_owned())
            .unwrap_or_else(|| fallback.to_owned())
    };
    Ok(Track {
        path: fs::canonicalize(path).unwrap_or_else(|_| path.to_owned()),
        title: text(
            tag.and_then(|t| t.title()),
            path.file_stem()
                .and_then(|v| v.to_str())
                .unwrap_or("Untitled"),
        ),
        artist: text(tag.and_then(|t| t.artist()), "Unknown artist"),
        album: text(tag.and_then(|t| t.album()), "Unknown album"),
        genre: text(tag.and_then(|t| t.genre()), "Unspecified"),
        number: tag.and_then(|t| t.track()).unwrap_or(0),
        seconds: f.properties().duration().as_secs(),
        format: path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_uppercase(),
    })
}
fn clock(s: u64) -> String {
    format!("{}:{:02}", s / 60, s % 60)
}
fn state_path() -> PathBuf {
    std::env::var_os("BLUETUNES_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share")
                })
                .join("bluetunes")
        })
        .join("library.json")
}
struct Player {
    library: Library,
    state: PathBuf,
    stream: Option<OutputStream>,
    sink: Option<Sink>,
    current: Option<usize>,
    selected: Option<usize>,
    checked: BTreeSet<usize>,
    queue: Vec<usize>,
    history: Vec<usize>,
    search: String,
    filters: [String; 3],
    playlist: Option<usize>,
    playlist_name: String,
    sort: usize,
    descending: bool,
    playlist_order: bool,
    volume: f32,
    shuffle: bool,
    repeat: bool,
    scan: Option<mpsc::Receiver<Result<Track, String>>>,
    status: String,
    imported: usize,
    skipped: usize,
    show_queue: bool,
    visualizer: visualizer::Visualizer,
    meter: visualizer::Meter,
    saved_settings: Settings,
    artwork_path: Option<PathBuf>,
    artwork_rx: Option<mpsc::Receiver<Option<egui::ColorImage>>>,
    artwork_texture: Option<egui::TextureHandle>,
}
impl Player {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::with_state(cc, state_path())
    }
    fn with_state(cc: &eframe::CreationContext<'_>, state: PathBuf) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = PANEL;
        style.visuals.window_fill = PANEL;
        style.visuals.extreme_bg_color = BG;
        style.visuals.override_text_color = Some(Color32::from_rgb(242, 247, 255));
        style.visuals.selection.bg_fill = Color32::from_rgb(30, 70, 110);
        style.visuals.selection.stroke.color = CYAN;
        style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(17, 29, 44);
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(33, 52, 76);
        style.spacing.item_spacing = egui::vec2(10., 8.);
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(15.));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(11.));
        cc.egui_ctx.set_style(style);
        let (library, mut status): (Library, String) = match fs::read(&state) { Ok(b) => match serde_json::from_slice(&b) { Ok(l) => (l,"Ready".into()), Err(e) => (Library::default(),format!("Library could not be read: {e}. Original file preserved until you import or save.")) }, Err(e) if e.kind()==std::io::ErrorKind::NotFound => (Library::default(),"Add a music folder to get started".into()), Err(e) => (Library::default(),format!("Cannot read library: {e}")) };
        let saved_settings =
            Settings::load(&state.with_file_name("settings.json")).unwrap_or_else(|e| {
                status = format!("Could not load playback settings: {e}");
                Settings::default()
            });
        let queue = saved_settings
            .queue
            .iter()
            .filter_map(|path| library.tracks.iter().position(|t| &t.path == path))
            .collect();
        let selected = (!library.tracks.is_empty()).then_some(0);
        Self {
            library,
            state,
            stream: None,
            sink: None,
            current: None,
            selected,
            checked: BTreeSet::new(),
            queue,
            history: vec![],
            search: String::new(),
            filters: Default::default(),
            playlist: None,
            playlist_name: String::new(),
            sort: 0,
            descending: false,
            playlist_order: true,
            volume: saved_settings.volume,
            shuffle: saved_settings.shuffle,
            repeat: saved_settings.repeat,
            scan: None,
            status,
            imported: 0,
            skipped: 0,
            show_queue: false,
            visualizer: Default::default(),
            meter: visualizer::meter(),
            saved_settings,
            artwork_path: None,
            artwork_rx: None,
            artwork_texture: None,
        }
    }
    fn add_to_playlist(&mut self, playlist: usize, ids: &[usize]) {
        let paths = ids
            .iter()
            .map(|id| self.library.tracks[*id].path.clone())
            .collect::<Vec<_>>();
        let count = self.library.playlists[playlist].add_paths(paths);
        self.status = if count == 0 {
            format!(
                "Songs are already in {}",
                self.library.playlists[playlist].name
            )
        } else {
            format!(
                "Added {count} song(s) to {}",
                self.library.playlists[playlist].name
            )
        };
        self.save();
    }
    fn persist_settings(&mut self) {
        let settings = Settings {
            volume: self.volume,
            shuffle: self.shuffle,
            repeat: self.repeat,
            queue: self
                .queue
                .iter()
                .map(|id| self.library.tracks[*id].path.clone())
                .collect(),
        };
        if settings != self.saved_settings {
            match settings.save(&self.state.with_file_name("settings.json")) {
                Ok(()) => self.saved_settings = settings,
                Err(e) => self.status = format!("Could not save playback settings: {e}"),
            }
        }
    }
    fn update_artwork(&mut self, ctx: &egui::Context) {
        let path = self
            .selected
            .or(self.current)
            .map(|id| self.library.tracks[id].path.clone());
        if path != self.artwork_path {
            self.artwork_path = path.clone();
            self.artwork_texture = None;
            self.artwork_rx = None;
            if let Some(path) = path {
                let (tx, rx) = mpsc::channel();
                self.artwork_rx = Some(rx);
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(artwork::load(&path));
                    ctx.request_repaint();
                });
            }
        }
        if let Some(rx) = self.artwork_rx.take() {
            match rx.try_recv() {
                Ok(Some(image)) => {
                    self.artwork_texture =
                        Some(ctx.load_texture("album_art", image, egui::TextureOptions::LINEAR))
                }
                Err(mpsc::TryRecvError::Empty) => self.artwork_rx = Some(rx),
                _ => {}
            }
        }
    }
    fn save(&mut self) {
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            fs::create_dir_all(self.state.parent().unwrap())?;
            if let Ok(bytes) = fs::read(&self.state) {
                if serde_json::from_slice::<Library>(&bytes).is_err() {
                    let backup = self.state.with_extension(format!(
                        "unreadable-{}.json",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)?
                            .as_nanos()
                    ));
                    fs::copy(&self.state, backup)?;
                }
            }
            let tmp = self.state.with_extension("json.tmp");
            fs::write(&tmp, serde_json::to_vec_pretty(&self.library)?)?;
            fs::rename(tmp, &self.state)?;
            Ok(())
        })();
        if let Err(e) = result {
            self.status = format!("Could not save library: {e}");
        }
    }
    fn import(&mut self, paths: Vec<PathBuf>) {
        if self.scan.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.scan = Some(rx);
        self.imported = 0;
        self.skipped = 0;
        self.status = "Reading music tags…".into();
        std::thread::spawn(move || {
            for root in paths {
                for entry in walkdir::WalkDir::new(root).into_iter() {
                    match entry {
                        Ok(e) if e.file_type().is_file() && supported(e.path()) => {
                            if tx.send(read_track(e.path())).is_err() {
                                return;
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Err(e.to_string()));
                        }
                        _ => {}
                    }
                }
            }
        });
    }
    fn visible(&self) -> Vec<usize> {
        let q = self.search.to_lowercase();
        let mut ids: Vec<_> = self
            .library
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                browser::matches(
                    t,
                    &self.filters,
                    3,
                    &q,
                    self.playlist
                        .map(|p| self.library.playlists[p].paths.as_slice()),
                )
            })
            .map(|(i, _)| i)
            .collect();
        if let Some(p) = self.playlist.filter(|_| self.playlist_order) {
            self.library.playlists[p].order_ids(&self.library.tracks, &mut ids);
            return ids;
        }
        ids.sort_by(|a, b| {
            let a = &self.library.tracks[*a];
            let b = &self.library.tracks[*b];
            let order = match self.sort {
                0 => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
                1 => a.artist.cmp(&b.artist),
                2 => a.album.cmp(&b.album).then(a.number.cmp(&b.number)),
                3 => a.seconds.cmp(&b.seconds),
                4 => a.genre.cmp(&b.genre),
                5 => a.format.cmp(&b.format),
                _ => a.number.cmp(&b.number),
            };
            if self.descending {
                order.reverse()
            } else {
                order
            }
        });
        ids
    }
    fn play(&mut self, id: usize) {
        let result = (|| -> Result<(), String> {
            let file = fs::File::open(&self.library.tracks[id].path).map_err(|e| e.to_string())?;
            let source = Decoder::try_from(file).map_err(|e| e.to_string())?;
            if self.stream.is_none() {
                self.stream =
                    Some(OutputStreamBuilder::open_default_stream().map_err(|e| e.to_string())?);
            }
            let sink = Sink::connect_new(self.stream.as_ref().unwrap().mixer());
            sink.set_volume(self.volume);
            if let Some(old) = self.sink.take() {
                old.stop();
            }
            self.meter = visualizer::meter();
            sink.append(visualizer::Tap::new(source, self.meter.clone()));
            self.sink = Some(sink);
            if let Some(old) = self.current {
                if old != id {
                    self.history.push(old);
                }
            }
            self.current = Some(id);
            self.selected = Some(id);
            self.status = format!("Playing {}", self.library.tracks[id].title);
            Ok(())
        })();
        if let Err(e) = result {
            if let Some(s) = self.sink.take() {
                s.stop();
            }
            self.status = format!("Playback failed: {e}");
        }
    }
    fn next(&mut self) {
        if !self.queue.is_empty() {
            let id = self.queue.remove(0);
            self.play(id);
            return;
        }
        let mut ids = self.visible();
        if let Some(p) = self.playlist {
            self.library.playlists[p].order_ids(&self.library.tracks, &mut ids);
        }
        if ids.is_empty() {
            self.sink = None;
            self.status = "Playback finished".into();
            return;
        }
        let next = if self.shuffle {
            let choices: Vec<_> = ids
                .iter()
                .copied()
                .filter(|i| Some(*i) != self.current)
                .collect();
            if choices.is_empty() {
                if self.repeat {
                    Some(ids[0])
                } else {
                    None
                }
            } else {
                Some(choices[rand::random_range(0..choices.len())])
            }
        } else {
            match self.current.and_then(|c| ids.iter().position(|i| *i == c)) {
                Some(p) if p + 1 < ids.len() => Some(ids[p + 1]),
                Some(_) if self.repeat => Some(ids[0]),
                Some(_) => None,
                None => Some(ids[0]),
            }
        };
        if let Some(id) = next {
            self.play(id);
        } else {
            self.sink = None;
            self.status = "Playback finished".into();
        }
    }
    fn toggle(&mut self) {
        if let Some(s) = &self.sink {
            if s.is_paused() {
                s.play()
            } else {
                s.pause()
            }
        } else if let Some(id) = self.selected.or_else(|| self.visible().first().copied()) {
            self.play(id);
        }
    }
}
impl eframe::App for Player {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(200));
        self.update_artwork(ctx);
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.visualizer.visible = false;
        }
        if let Some(rx) = self.scan.take() {
            let mut done = false;
            for _ in 0..1000 {
                match rx.try_recv() {
                    Ok(Ok(t)) => {
                        if !self.library.tracks.iter().any(|x| x.path == t.path) {
                            self.library.tracks.push(t);
                            self.imported += 1;
                        }
                    }
                    Ok(Err(_)) => self.skipped += 1,
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        done = true;
                        break;
                    }
                }
            }
            if done {
                self.status = format!(
                    "Imported {} tracks · {} unreadable files or folders",
                    self.imported, self.skipped
                );
                self.save();
            } else {
                self.scan = Some(rx);
            }
        }
        let dropped: Vec<_> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if !dropped.is_empty() {
            self.import(dropped);
        }
        if self.sink.as_ref().is_some_and(|s| s.empty()) {
            self.next();
        }
        if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Space)) {
            self.toggle();
        }
        egui::TopBottomPanel::top("transport").show(ctx, |ui| {
            ui.add_space(10.);
            ui.horizontal(|ui| {
                ui.label(RichText::new("BLUE\nTUNES").size(20.).strong().color(BLUE));
                ui.add_space(22.);
                if ui.button("|◀").on_hover_text("Previous track").clicked() {
                    if let Some(id) = self.history.pop() {
                        self.current = None;
                        self.play(id);
                    }
                }
                let playing = self.sink.as_ref().is_some_and(|s| !s.is_paused());
                if ui
                    .add_sized(
                        [70., 36.],
                        egui::Button::new(if playing { "Pause" } else { "Play" }),
                    )
                    .clicked()
                {
                    self.toggle();
                }
                if ui.button("▶|").on_hover_text("Next track").clicked() {
                    self.next();
                }
                ui.add_space(18.);
                ui.vertical(|ui| {
                    ui.set_width((ui.available_width() - 290.).max(220.));
                    if let Some(id) = self.current {
                        let t = &self.library.tracks[id];
                        ui.label(RichText::new(&t.title).strong().color(CYAN));
                        ui.label(
                            RichText::new(format!("{}  /  {}", t.artist, t.album)).color(MUTED),
                        );
                        let mut pos = self
                            .sink
                            .as_ref()
                            .map(|s| s.get_pos().as_secs_f64())
                            .unwrap_or(0.);
                        let length = t.seconds.max(1) as f64;
                        ui.horizontal(|ui| {
                            ui.label(clock(pos as u64));
                            let r =
                                ui.add(egui::Slider::new(&mut pos, 0.0..=length).show_value(false));
                            if r.changed() {
                                if let Some(s) = &self.sink {
                                    if let Err(e) = s.try_seek(Duration::from_secs_f64(pos)) {
                                        self.status = format!("Seek failed: {e}");
                                    }
                                }
                            }
                            ui.label(clock(t.seconds));
                        });
                    } else {
                        ui.label(
                            RichText::new("Your music. In good order.")
                                .strong()
                                .color(CYAN),
                        );
                        ui.label(
                            RichText::new("Select a song and press Play · MP3 + FLAC").color(MUTED),
                        );
                    }
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.toggle_value(&mut self.shuffle, "Shuffle");
                        ui.toggle_value(&mut self.repeat, "Repeat all");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Volume");
                        if ui
                            .add(egui::Slider::new(&mut self.volume, 0.0..=1.).show_value(false))
                            .changed()
                        {
                            if let Some(s) = &self.sink {
                                s.set_volume(self.volume);
                            }
                        }
                    });
                });
            });
            ui.add_space(6.);
            ui.toggle_value(&mut self.visualizer.visible, "Visualizer")
                .on_hover_text("Show audio-reactive patterns");
            ui.add_space(8.);
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.scan.is_some() {
                    ui.spinner();
                }
                ui.label(RichText::new(&self.status).color(MUTED));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!(
                        "{} songs  ·  {} total",
                        self.library.tracks.len(),
                        clock(self.library.tracks.iter().map(|t| t.seconds).sum())
                    ));
                });
            });
        });
        egui::SidePanel::left("sidebar").resizable(false).exact_width(190.).show(ctx,|ui|{
            ui.add_space(20.);ui.label(RichText::new("LIBRARY").small().color(CYAN));
            if ui.selectable_label(self.playlist.is_none()&&!self.show_queue,"♫  All music").clicked(){self.playlist=None;self.show_queue=false;self.filters=Default::default();}
            if ui.selectable_label(self.show_queue,format!("≡  Up next ({})",self.queue.len())).clicked(){self.show_queue=!self.show_queue;}
            ui.add_space(12.);
            if ui.add_enabled(self.scan.is_none(),egui::Button::new("+ Add music folder")).clicked(){if let Some(p)=rfd::FileDialog::new().pick_folder(){self.import(vec![p]);}}
            if ui.add_enabled(self.scan.is_none(),egui::Button::new("+ Add files")).clicked(){if let Some(p)=rfd::FileDialog::new().add_filter("Music",&["mp3","flac"]).pick_files(){self.import(p);}}
            ui.add_space(28.);ui.label(RichText::new("PLAYLISTS").small().color(CYAN));
            for (i,p) in self.library.playlists.iter().enumerate(){if ui.selectable_label(self.playlist==Some(i)&&!self.show_queue,format!("♪  {}",p.name)).clicked(){self.playlist=Some(i);self.playlist_order=true;self.show_queue=false;self.filters=Default::default();}}
            ui.add_space(12.);ui.add(egui::TextEdit::singleline(&mut self.playlist_name).hint_text("New playlist").desired_width(160.));
            if ui.add_enabled(!self.playlist_name.trim().is_empty(),egui::Button::new("Create playlist")).clicked(){self.library.playlists.push(Playlist{name:self.playlist_name.trim().into(),paths:vec![]});self.playlist_name.clear();self.save();}
            ui.add_space(20.);
            if let Some(texture)=&self.artwork_texture {
                ui.add(egui::Image::new(texture).fit_to_exact_size(egui::vec2(170.,170.)).maintain_aspect_ratio(true));
            } else {
                let (rect,_)=ui.allocate_exact_size(egui::vec2(170.,130.),egui::Sense::hover());
                ui.painter().rect_filled(rect,8.,Color32::from_rgb(17,29,44));
                ui.painter().circle_stroke(rect.center(),43.,egui::Stroke::new(2.0_f32,BLUE));
                ui.painter().circle_stroke(rect.center(),31.,egui::Stroke::new(1.0_f32,MUTED));
                ui.painter().circle_filled(rect.center(),8.,CYAN);
            }
            if let Some(id)=self.selected.or(self.current) {
                ui.add(egui::Label::new(RichText::new(&self.library.tracks[id].album).color(CYAN)).truncate());
                ui.add(egui::Label::new(RichText::new(&self.library.tracks[id].artist).small().color(MUTED)).truncate());
            }
            ui.add_space(25.);ui.label(RichText::new("Double-click to play\nCheck songs, then use\nAdd to playlist above\nthe song list\n\nSpace to play / pause\nDrop music files here").small().color(MUTED));
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).fill(BG))
            .show(ctx, |ui| {
                if self.visualizer.visible {
                    let playing = self
                        .sink
                        .as_ref()
                        .is_some_and(|s| !s.is_paused() && !s.empty());
                    let title = self
                        .current
                        .map(|id| {
                            format!(
                                "{} — {}",
                                self.library.tracks[id].artist, self.library.tracks[id].title
                            )
                        })
                        .unwrap_or_default();
                    self.visualizer.show(ui, &self.meter, playing, &title);
                    return;
                }
                ui.add_space(12.);
                ui.horizontal(|ui| {
                    ui.heading(if self.show_queue {
                        "Up next"
                    } else {
                        self.playlist
                            .map(|p| self.library.playlists[p].name.as_str())
                            .unwrap_or("Music")
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Clear filters").clicked() {
                            self.search.clear();
                            self.filters = Default::default();
                        }
                        ui.add(
                            egui::TextEdit::singleline(&mut self.search)
                                .hint_text("Search your music…")
                                .desired_width(230.),
                        );
                    });
                });
                ui.add_space(12.);
                if !self.show_queue {
                    ui.columns(3, |cols| {
                        for (n, label) in ["GENRE", "ARTIST", "ALBUM"].iter().enumerate() {
                            cols[n].label(RichText::new(*label).small().color(CYAN));
                            egui::ScrollArea::vertical()
                                .id_salt(n)
                                .max_height(115.)
                                .show(&mut cols[n], |ui| {
                                    if ui
                                        .selectable_label(self.filters[n].is_empty(), "All")
                                        .clicked()
                                    {
                                        browser::select(&mut self.filters,n,String::new());
                                    }
                                    let values = browser::values(&self.library.tracks,&self.filters,n,&self.search.to_lowercase(),self.playlist.map(|p|self.library.playlists[p].paths.as_slice()));
                                    for v in values {
                                        if ui.selectable_label(self.filters[n] == v, &v).clicked() {
                                            browser::select(&mut self.filters,n,v);
                                        }
                                    }
                                });
                        }
                    });
                    ui.add_space(12.);
                    ui.separator();
                }
                if self.playlist.is_some() && !self.show_queue {
                    ui.horizontal_wrapped(|ui| {
                        if ui.selectable_label(self.playlist_order,"Playlist order").clicked(){self.playlist_order=true;}
                        ui.label(if self.playlist_order {"Drag the numbered handle to move a song. Drop above or below a row."} else {"Choose Playlist order to rearrange songs."});
                    });
                }
                let reorder_enabled=self.playlist.is_some() && self.playlist_order && !self.show_queue;
                let ids = if self.show_queue {
                    self.queue.clone()
                } else {
                    self.visible()
                };
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{} songs", ids.len()))
                            .small()
                            .color(MUTED),
                    );
                    if self.show_queue && ui.button("Clear queue").clicked() {
                        self.queue.clear();
                    }
                });
                let chosen: Vec<usize>=ids.iter().copied().filter(|id| if self.checked.is_empty(){self.selected==Some(*id)}else{self.checked.contains(id)}).collect();
                let mut destination=None;
                let mut create_and_add=false;
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("{} selected",chosen.len()));
                    if ui.button("Select all shown").clicked(){self.checked=ids.iter().copied().collect();}
                    if ui.button("Clear selection").clicked(){self.checked.clear();self.selected=None;}
                    ui.add_enabled_ui(!chosen.is_empty(),|ui| {
                        ui.menu_button("Add to playlist…",|ui| {
                            for (index,playlist) in self.library.playlists.iter().enumerate() {
                                if ui.button(&playlist.name).clicked(){destination=Some(index);ui.close_menu();}
                            }
                            ui.separator();
                            ui.label("New playlist");
                            ui.add(egui::TextEdit::singleline(&mut self.playlist_name).hint_text("Playlist name"));
                            if ui.add_enabled(!self.playlist_name.trim().is_empty(),egui::Button::new("Create and add songs")).clicked(){create_and_add=true;ui.close_menu();}
                        });
                    });
                });
                if create_and_add {
                    self.library.playlists.push(Playlist{name:self.playlist_name.trim().into(),paths:vec![]});
                    self.playlist_name.clear();destination=Some(self.library.playlists.len()-1);
                }
                if let Some(p)=destination {self.add_to_playlist(p,&chosen);}
                if self.library.tracks.is_empty() {
                    ui.add_space(60.);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("A home for your music.")
                                .size(28.)
                                .color(BLUE),
                        );
                        ui.label("Add a folder of MP3 or FLAC files to build your library.");
                        ui.label(
                            RichText::new("Your files stay right where they are.").color(MUTED),
                        );
                    });
                    return;
                }
                if ids.is_empty() && !self.show_queue {
                    ui.add_space(24.);
                    ui.label("No songs match these filters. Use Clear filters to show all songs in this library or playlist.");
                    return;
                }
                let mut play = None;
                let mut enqueue = None;
                let mut add = None;
                let mut remove = None;
                let mut reorder = None;
                egui::ScrollArea::horizontal()
                    .id_salt("track_columns")
                    .show(ui, |ui| {
                        ui.set_min_width(980.);
                        TableBuilder::new(ui)
                            .striped(true)
                            .resizable(true)
                            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                            .column(Column::exact(28.))
                            .column(Column::exact(58.))
                            .column(Column::initial(250.).at_least(100.))
                            .column(Column::initial(170.).at_least(70.))
                            .column(Column::initial(190.).at_least(70.))
                            .column(Column::initial(65.))
                            .column(Column::initial(100.))
                            .column(Column::initial(65.))
                            .column(Column::remainder().at_least(35.))
                            .header(30., |mut row| {
                                row.col(|ui|{ui.label("✓");});
                                row.col(|ui|{ui.label(if self.playlist.is_some(){"ORDER"}else{""});});
                                for (i, h) in
                                    ["TITLE", "ARTIST", "ALBUM", "TIME", "GENRE", "FORMAT", "#"]
                                        .iter()
                                        .enumerate()
                                {
                                    row.col(|ui| {
                                        let label = format!(
                                            "{}{}",
                                            h,
                                            if self.sort == i && !(self.playlist.is_some() && self.playlist_order) {
                                                if self.descending {
                                                    " -"
                                                } else {
                                                    " +"
                                                }
                                            } else {
                                                ""
                                            }
                                        );
                                        if ui
                                            .button(RichText::new(label).small().color(BLUE))
                                            .clicked()
                                        {
                                            self.playlist_order=false;
                                            if self.sort == i {
                                                self.descending = !self.descending;
                                            } else {
                                                self.sort = i;
                                                self.descending = false;
                                            }
                                        }
                                    });
                                }
                            })
                            .body(|body| {
                                body.rows(29., ids.len(), |mut row| {
                                    let id = ids[row.index()];
                                    let t = &self.library.tracks[id];
                                    row.set_selected(self.checked.contains(&id) || self.selected == Some(id));
                                    row.col(|ui| {
                                        let mut checked=self.checked.contains(&id);
                                        if ui.checkbox(&mut checked, "").changed(){
                                            if checked {self.checked.insert(id);}else{self.checked.remove(&id);}
                                            self.selected=None;
                                        }
                                    });
                                    row.col(|ui| {
                                        if let Some(p)=self.playlist.filter(|_|!self.show_queue) {
                                            let position=self.library.playlists[p].paths.iter().position(|path|path==&t.path).map(|n|n+1).unwrap_or(0);
                                            if reorder_enabled {
                                                ui.dnd_drag_source(egui::Id::new(("playlist_drag",p,id)),PlaylistDrag{playlist:p,path:t.path.clone()},|ui|{
                                                    ui.label(RichText::new(format!("≡ {position}")).color(BLUE));
                                                }).response.on_hover_text("Drag to change play position");
                                            } else {ui.label(position.to_string());}
                                        }
                                    });
                                    row.col(|ui| {
                                        let label = if self.current == Some(id) {
                                            format!("▶  {}", t.title)
                                        } else {
                                            t.title.clone()
                                        };
                                        let r = ui.selectable_label(
                                            self.selected == Some(id),
                                            RichText::new(label).color(
                                                if self.current == Some(id) {
                                                    CYAN
                                                } else {
                                                    Color32::from_rgb(242, 247, 255)
                                                },
                                            ),
                                        );
                                        if r.clicked() {
                                            self.checked.clear();
                                            self.selected = Some(id);
                                        }
                                        if r.double_clicked() {
                                            play = Some(id);
                                        }
                                        r.context_menu(|ui| {
                                            if ui.button("Play now").clicked() {
                                                play = Some(id);
                                                ui.close_menu();
                                            }
                                            if ui.button("Add to Up next").clicked() {
                                                enqueue = Some(id);
                                                ui.close_menu();
                                            }
                                            for (p, pl) in self.library.playlists.iter().enumerate()
                                            {
                                                if ui
                                                    .button(format!("Add to {}", pl.name))
                                                    .clicked()
                                                {
                                                    add = Some((p, id));
                                                    ui.close_menu();
                                                }
                                            }
                                            if let Some(p) = self.playlist {
                                                if ui.button("Remove from this playlist").clicked()
                                                {
                                                    remove = Some((p, id));
                                                    ui.close_menu();
                                                }
                                            }
                                        });
                                    });
                                    for text in [
                                        &t.artist,
                                        &t.album,
                                        &clock(t.seconds),
                                        &t.genre,
                                        &t.format,
                                        &t.number.to_string(),
                                    ] {
                                        row.col(|ui| {
                                            ui.add(
                                                egui::Label::new(RichText::new(text).color(MUTED))
                                                    .truncate(),
                                            );
                                        });
                                    }
                                    if reorder_enabled {
                                        let response=row.response();
                                        let after=response.ctx.input(|i|i.pointer.hover_pos().is_some_and(|pos|pos.y>response.rect.center().y));
                                        if let Some(payload)=response.dnd_hover_payload::<PlaylistDrag>() {
                                            if Some(payload.playlist)==self.playlist && payload.path!=t.path {
                                                let y=if after{response.rect.bottom()}else{response.rect.top()};
                                                response.ctx.layer_painter(response.layer_id).line_segment([egui::pos2(response.rect.left(),y),egui::pos2(response.rect.right(),y)],egui::Stroke::new(2.0_f32,CYAN));
                                            }
                                        }
                                        if let Some(payload)=response.dnd_release_payload::<PlaylistDrag>() {
                                            if Some(payload.playlist)==self.playlist {reorder=Some((payload.playlist,payload.path.clone(),t.path.clone(),after));}
                                        }
                                    }
                                });
                            });
                    });
                if let Some((p,source,target,after))=reorder {
                    if self.library.playlists[p].move_relative(&source,&target,after) {
                        self.status="Playlist order saved".into();self.save();
                    }
                }
                if let Some(id) = play {
                    self.play(id);
                }
                if let Some(id) = enqueue {
                    self.queue.push(id);
                }
                if let Some((p, id)) = add {
                    self.add_to_playlist(p,&[id]);
                }
                if let Some((p, id)) = remove {
                    self.library.playlists[p]
                        .paths
                        .retain(|x| x != &self.library.tracks[id].path);
                    self.save();
                }
            });
        self.persist_settings();
    }
}
fn main() -> eframe::Result {
    eframe::run_native(
        "BlueTunes",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_app_id("com.bluetunes.Player")
                .with_inner_size([1280., 800.])
                .with_min_inner_size([1000., 600.]),
            ..Default::default()
        },
        Box::new(|cc| {
            let mut player = Player::new(cc);
            player.visualizer.visible = std::env::args().any(|arg| arg == "--visualizer");
            Ok(Box::new(player))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn playlist_mouse_drag_saves_new_order() {
        use eframe::App;
        let ctx = egui::Context::default();
        let dir = std::env::temp_dir().join(format!("bluetunes-drag-{}", std::process::id()));
        let mut player = Player::with_state(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            dir.join("library.json"),
        );
        player.library.tracks = ["First", "Second", "Third"]
            .iter()
            .map(|name| Track {
                path: PathBuf::from(format!("{name}.mp3")),
                title: name.to_string(),
                artist: "Artist".into(),
                album: "Album".into(),
                genre: "Rock".into(),
                number: 1,
                seconds: 10,
                format: "MP3".into(),
            })
            .collect();
        player.library.playlists = vec![Playlist {
            name: "Test order".into(),
            paths: player
                .library
                .tracks
                .iter()
                .map(|t| t.path.clone())
                .collect(),
        }];
        player.playlist = Some(0);
        let mut frame = eframe::Frame::_new_kittest();
        let mut render = |player: &mut Player, events: Vec<egui::Event>| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280., 800.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| player.update(ctx, &mut frame),
            )
        };
        render(&mut player, vec![]);
        let output = render(&mut player, vec![]);
        let find = |text: &str| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(t) if t.galley.text() == text => {
                        Some(t.pos + t.galley.size() * 0.5)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("Missing widget {text}"))
        };
        let source = find("≡ 1");
        let target = find("Third") + egui::vec2(0., 7.);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        render(&mut player, vec![egui::Event::PointerMoved(source)]);
        render(&mut player, vec![button(source, true)]);
        render(
            &mut player,
            vec![egui::Event::PointerMoved(source + egui::vec2(0., 12.))],
        );
        render(&mut player, vec![egui::Event::PointerMoved(target)]);
        render(&mut player, vec![]);
        render(&mut player, vec![button(target, false)]);
        assert_eq!(
            player.library.playlists[0].paths,
            ["Second.mp3", "Third.mp3", "First.mp3"].map(PathBuf::from)
        );
        let saved: Library =
            serde_json::from_slice(&fs::read(dir.join("library.json")).unwrap()).unwrap();
        assert_eq!(saved.playlists[0].paths, player.library.playlists[0].paths);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn playlist_reorders_up_down_and_persists() {
        let mut p = Playlist {
            name: "Order".into(),
            paths: ["a", "b", "c", "d"].map(PathBuf::from).to_vec(),
        };
        assert!(p.move_relative(Path::new("a"), Path::new("d"), true));
        assert_eq!(p.paths, ["b", "c", "d", "a"].map(PathBuf::from));
        assert!(p.move_relative(Path::new("a"), Path::new("b"), false));
        assert_eq!(p.paths, ["a", "b", "c", "d"].map(PathBuf::from));
        assert!(p.move_relative(Path::new("d"), Path::new("b"), false));
        assert_eq!(p.paths, ["a", "d", "b", "c"].map(PathBuf::from));
        assert!(!p.move_relative(Path::new("d"), Path::new("d"), true));
        assert!(!p.move_relative(Path::new("absent"), Path::new("a"), false));
        let restored: Playlist = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
        assert_eq!(restored.paths, p.paths);
    }
    #[test]
    fn playlist_playback_and_filtered_views_use_saved_order() {
        let tracks: Vec<_> = ["a", "b", "c", "d"]
            .iter()
            .map(|path| Track {
                path: PathBuf::from(path),
                title: path.to_string(),
                artist: "Artist".into(),
                album: "Album".into(),
                genre: "Genre".into(),
                number: 1,
                seconds: 1,
                format: "MP3".into(),
            })
            .collect();
        let mut p = Playlist {
            name: "Order".into(),
            paths: ["a", "b", "c", "d"].map(PathBuf::from).to_vec(),
        };
        // A filtered view showing a and d can reorder them without deleting hidden b/c.
        p.move_relative(Path::new("d"), Path::new("a"), false);
        let mut all = vec![0, 1, 2, 3];
        p.order_ids(&tracks, &mut all);
        assert_eq!(all, vec![3, 0, 1, 2]);
        let mut filtered = vec![0, 3];
        p.order_ids(&tracks, &mut filtered);
        assert_eq!(filtered, vec![3, 0]);
        assert_eq!(p.paths.len(), 4);
    }
    #[test]
    fn playlist_bulk_add_deduplicates_and_survives_reload() {
        let mut playlist = Playlist {
            name: "Favorites".into(),
            paths: vec!["one.mp3".into()],
        };
        assert_eq!(
            playlist.add_paths(vec![
                "one.mp3".into(),
                "two.flac".into(),
                "two.flac".into(),
                "three.mp3".into()
            ]),
            2
        );
        let bytes = serde_json::to_vec(&playlist).unwrap();
        let restored: Playlist = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored.name, "Favorites");
        assert_eq!(
            restored.paths,
            vec![
                PathBuf::from("one.mp3"),
                PathBuf::from("two.flac"),
                PathBuf::from("three.mp3")
            ]
        );
    }

    #[test]
    #[ignore = "Requires a desktop audio device and generated fixtures"]
    fn audio_device_play_pause_seek() {
        let dir = PathBuf::from(std::env::var_os("BLUETUNES_TEST_AUDIO").expect("audio fixtures"));
        let stream = OutputStreamBuilder::open_default_stream().unwrap();
        for ext in ["mp3", "flac"] {
            let sink = Sink::connect_new(stream.mixer());
            sink.set_volume(0.0);
            sink.append(visualizer::Tap::new(
                Decoder::try_from(fs::File::open(dir.join(format!("test.{ext}"))).unwrap())
                    .unwrap(),
                visualizer::meter(),
            ));
            std::thread::sleep(Duration::from_millis(150));
            assert!(!sink.empty());
            assert!(sink.get_pos() > Duration::ZERO);
            sink.pause();
            assert!(sink.is_paused());
            sink.try_seek(Duration::from_secs(1)).unwrap();
            sink.play();
            assert!(!sink.is_paused());
            sink.sleep_until_end();
            assert!(sink.empty());
        }
    }
    #[test]
    fn formats_are_case_insensitive() {
        assert!(supported(Path::new("track.FLAC")));
        assert!(supported(Path::new("track.Mp3")));
        assert!(!supported(Path::new("track.txt")));
    }
    #[test]
    #[ignore = "Requires generated audio fixtures"]
    fn reads_and_decodes_real_audio() {
        let dir = std::env::var_os("BLUETUNES_TEST_AUDIO").expect("audio fixtures");
        use rodio::Source;
        for ext in ["mp3", "flac"] {
            let p = PathBuf::from(&dir).join(format!("test.{ext}"));
            let t = read_track(&p).unwrap();
            assert_eq!(t.title, "Test Song");
            assert_eq!(t.artist, "Test Artist");
            assert_eq!(t.album, "Test Album");
            let mut d = Decoder::try_from(fs::File::open(p).unwrap()).unwrap();
            assert!(d.sample_rate() > 0);
            assert!(d.any(|x| x.abs() > 0.001));
        }
    }
}
