use crate::account_manager::AccountManager;
use crate::app_creation::setup_cc;
use crate::app_style::user_requested_visuals_change;
use crate::error::Error;
use crate::frame_history::FrameHistory;
use crate::imgcache::ImageCache;
use crate::notecache::{CachedNote, NoteCache};
use crate::route::Route;
use crate::timeline;
use crate::timeline::{NoteRef, Timeline, ViewFilter};
use crate::ui::profile::SimpleProfilePreviewController;
use crate::ui::{is_mobile, DesktopSidePanel};
use crate::Result;

use egui::{Context, Frame, Style};
use egui_extras::{Size, StripBuilder};

use enostr::{ClientMessage, Filter, Pubkey, RelayEvent, RelayMessage};
use nostrdb::{BlockType, Config, Mention, Ndb, Note, NoteKey, Transaction};

use std::collections::HashSet;
use std::hash::Hash;
use std::path::Path;
use std::time::Duration;
use tracing::{debug, error, info, warn};

use enostr::RelayPool;

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum DamusState {
    Initializing,
    Initialized,
}

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
pub struct Damus {
    state: DamusState,
    //compose: String,
    note_cache: NoteCache,
    pool: RelayPool,

    /// global navigation for account management popups, etc.
    nav: Vec<Route>,
    pub textmode: bool,

    pub timelines: Vec<Timeline>,
    pub selected_timeline: i32,

    pub img_cache: ImageCache,
    pub ndb: Ndb,
    pub account_manager: AccountManager,

    frame_history: crate::frame_history::FrameHistory,
}

fn relay_setup(pool: &mut RelayPool, ctx: &egui::Context) {
    let ctx = ctx.clone();
    let wakeup = move || {
        ctx.request_repaint();
    };
    if let Err(e) = pool.add_url("ws://localhost:8080".to_string(), wakeup.clone()) {
        error!("{:?}", e)
    }
    if let Err(e) = pool.add_url("wss://relay.damus.io".to_string(), wakeup.clone()) {
        error!("{:?}", e)
    }
    //if let Err(e) = pool.add_url("wss://pyramid.fiatjaf.com".to_string(), wakeup.clone()) {
    //error!("{:?}", e)
    //}
    if let Err(e) = pool.add_url("wss://nos.lol".to_string(), wakeup.clone()) {
        error!("{:?}", e)
    }
    if let Err(e) = pool.add_url("wss://nostr.wine".to_string(), wakeup.clone()) {
        error!("{:?}", e)
    }
    if let Err(e) = pool.add_url("wss://purplepag.es".to_string(), wakeup) {
        error!("{:?}", e)
    }
}

fn since_optimize_filter(filter: &mut enostr::Filter, notes: &[NoteRef]) {
    // Get the latest entry in the events
    if notes.is_empty() {
        return;
    }

    // get the latest note
    let latest = notes[0];
    let since = latest.created_at - 60;

    // update the filters
    filter.since = Some(since);
}

fn send_initial_filters(damus: &mut Damus, relay_url: &str) {
    info!("Sending initial filters to {}", relay_url);
    let mut c: u32 = 1;

    for relay in &mut damus.pool.relays {
        let relay = &mut relay.relay;
        if relay.url == relay_url {
            for timeline in &damus.timelines {
                let mut filter = timeline.filter.clone();
                for f in &mut filter {
                    since_optimize_filter(f, timeline.notes(ViewFilter::NotesAndReplies));
                }
                relay.subscribe(format!("initial{}", c), filter);
                c += 1;
            }
            return;
        }
    }
}

enum ContextAction {
    SetPixelsPerPoint(f32),
}

fn handle_key_events(
    input: &egui::InputState,
    pixels_per_point: f32,
    damus: &mut Damus,
) -> Option<ContextAction> {
    let amount = 0.2;

    // We can't do things like setting the pixels_per_point when we are holding
    // on to an locked InputState context, so we need to pass actions externally
    let mut context_action: Option<ContextAction> = None;

    for event in &input.raw.events {
        if let egui::Event::Key {
            key, pressed: true, ..
        } = event
        {
            match key {
                egui::Key::Equals => {
                    context_action =
                        Some(ContextAction::SetPixelsPerPoint(pixels_per_point + amount));
                }
                egui::Key::Minus => {
                    context_action =
                        Some(ContextAction::SetPixelsPerPoint(pixels_per_point - amount));
                }
                egui::Key::J => {
                    damus.select_down();
                }
                egui::Key::K => {
                    damus.select_up();
                }
                egui::Key::H => {
                    damus.select_left();
                }
                egui::Key::L => {
                    damus.select_left();
                }
                _ => {}
            }
        }
    }

    context_action
}

fn try_process_event(damus: &mut Damus, ctx: &egui::Context) -> Result<()> {
    let ppp = ctx.pixels_per_point();
    let res = ctx.input(|i| handle_key_events(i, ppp, damus));
    if let Some(action) = res {
        match action {
            ContextAction::SetPixelsPerPoint(amt) => {
                ctx.set_pixels_per_point(amt);
            }
        }
    }

    let ctx2 = ctx.clone();
    let wakeup = move || {
        ctx2.request_repaint();
    };
    damus.pool.keepalive_ping(wakeup);

    // pool stuff
    while let Some(ev) = damus.pool.try_recv() {
        let relay = ev.relay.to_owned();

        match (&ev.event).into() {
            RelayEvent::Opened => send_initial_filters(damus, &relay),
            // TODO: handle reconnects
            RelayEvent::Closed => warn!("{} connection closed", &relay),
            RelayEvent::Error(e) => error!("wsev->relayev: {}", e),
            RelayEvent::Other(msg) => debug!("other event {:?}", &msg),
            RelayEvent::Message(msg) => process_message(damus, &relay, &msg),
        }
    }

    let txn = Transaction::new(&damus.ndb)?;
    let mut unknown_ids: HashSet<UnknownId> = HashSet::new();
    for timeline in 0..damus.timelines.len() {
        if let Err(err) = poll_notes_for_timeline(damus, &txn, timeline, &mut unknown_ids) {
            error!("{}", err);
        }
    }

    let unknown_ids: Vec<UnknownId> = unknown_ids.into_iter().collect();
    if let Some(filters) = get_unknown_ids_filter(&unknown_ids) {
        info!(
            "Getting {} unknown author profiles from relays",
            unknown_ids.len()
        );
        let msg = ClientMessage::req("unknown_ids".to_string(), filters);
        damus.pool.send(&msg);
    }

    Ok(())
}

#[derive(Hash, Clone, Copy, PartialEq, Eq)]
enum UnknownId<'a> {
    Pubkey(&'a [u8; 32]),
    Id(&'a [u8; 32]),
}

impl<'a> UnknownId<'a> {
    pub fn is_pubkey(&self) -> Option<&'a [u8; 32]> {
        match self {
            UnknownId::Pubkey(pk) => Some(pk),
            _ => None,
        }
    }

    pub fn is_id(&self) -> Option<&'a [u8; 32]> {
        match self {
            UnknownId::Id(id) => Some(id),
            _ => None,
        }
    }
}

fn get_unknown_note_ids<'a>(
    ndb: &Ndb,
    cached_note: &CachedNote,
    txn: &'a Transaction,
    note: &Note<'a>,
    note_key: NoteKey,
    ids: &mut HashSet<UnknownId<'a>>,
) -> Result<()> {
    // the author pubkey

    if ndb.get_profile_by_pubkey(txn, note.pubkey()).is_err() {
        ids.insert(UnknownId::Pubkey(note.pubkey()));
    }

    // pull notes that notes are replying to
    if cached_note.reply.root.is_some() {
        let note_reply = cached_note.reply.borrow(note.tags());
        if let Some(root) = note_reply.root() {
            if ndb.get_note_by_id(txn, root.id).is_err() {
                ids.insert(UnknownId::Id(root.id));
            }
        }

        if !note_reply.is_reply_to_root() {
            if let Some(reply) = note_reply.reply() {
                if ndb.get_note_by_id(txn, reply.id).is_err() {
                    ids.insert(UnknownId::Id(reply.id));
                }
            }
        }
    }

    let blocks = ndb.get_blocks_by_key(txn, note_key)?;
    for block in blocks.iter(note) {
        if block.blocktype() != BlockType::MentionBech32 {
            continue;
        }

        match block.as_mention().unwrap() {
            Mention::Pubkey(npub) => {
                if ndb.get_profile_by_pubkey(txn, npub.pubkey()).is_err() {
                    ids.insert(UnknownId::Pubkey(npub.pubkey()));
                }
            }
            Mention::Profile(nprofile) => {
                if ndb.get_profile_by_pubkey(txn, nprofile.pubkey()).is_err() {
                    ids.insert(UnknownId::Pubkey(nprofile.pubkey()));
                }
            }
            Mention::Event(ev) => match ndb.get_note_by_id(txn, ev.id()) {
                Err(_) => {
                    ids.insert(UnknownId::Id(ev.id()));
                    if let Some(pk) = ev.pubkey() {
                        if ndb.get_profile_by_pubkey(txn, pk).is_err() {
                            ids.insert(UnknownId::Pubkey(pk));
                        }
                    }
                }
                Ok(note) => {
                    if ndb.get_profile_by_pubkey(txn, note.pubkey()).is_err() {
                        ids.insert(UnknownId::Pubkey(note.pubkey()));
                    }
                }
            },
            Mention::Note(note) => match ndb.get_note_by_id(txn, note.id()) {
                Err(_) => {
                    ids.insert(UnknownId::Id(note.id()));
                }
                Ok(note) => {
                    if ndb.get_profile_by_pubkey(txn, note.pubkey()).is_err() {
                        ids.insert(UnknownId::Pubkey(note.pubkey()));
                    }
                }
            },
            _ => {}
        }
    }

    Ok(())
}

fn poll_notes_for_timeline<'a>(
    damus: &mut Damus,
    txn: &'a Transaction,
    timeline_ind: usize,
    ids: &mut HashSet<UnknownId<'a>>,
) -> Result<()> {
    let sub = if let Some(sub) = &damus.timelines[timeline_ind].subscription {
        sub
    } else {
        return Err(Error::NoActiveSubscription);
    };

    let new_note_ids = damus.ndb.poll_for_notes(sub, 100);
    if !new_note_ids.is_empty() {
        debug!("{} new notes! {:?}", new_note_ids.len(), new_note_ids);
    }

    let new_refs: Vec<(Note, NoteRef)> = new_note_ids
        .iter()
        .map(|key| {
            let note = damus.ndb.get_note_by_key(txn, *key).expect("no note??");
            let cached_note = damus
                .note_cache_mut()
                .cached_note_or_insert(*key, &note)
                .clone();
            let _ = get_unknown_note_ids(&damus.ndb, &cached_note, txn, &note, *key, ids);

            let created_at = note.created_at();
            (
                note,
                NoteRef {
                    key: *key,
                    created_at,
                },
            )
        })
        .collect();

    // ViewFilter::NotesAndReplies
    {
        let timeline = &mut damus.timelines[timeline_ind];

        let prev_items = timeline.notes(ViewFilter::NotesAndReplies).len();

        let refs: Vec<NoteRef> = new_refs.iter().map(|(_note, nr)| *nr).collect();
        timeline.view_mut(ViewFilter::NotesAndReplies).notes =
            timeline::merge_sorted_vecs(timeline.notes(ViewFilter::NotesAndReplies), &refs);

        let new_items = timeline.notes(ViewFilter::NotesAndReplies).len() - prev_items;

        // TODO: technically items could have been added inbetween
        if new_items > 0 {
            damus.timelines[timeline_ind]
                .view(ViewFilter::NotesAndReplies)
                .list
                .borrow_mut()
                .items_inserted_at_start(new_items);
        }
    }

    //
    // handle the filtered case (ViewFilter::Notes, no replies)
    //
    // TODO(jb55): this is mostly just copied from above, let's just use a loop
    //             I initially tried this but ran into borrow checker issues
    {
        let mut filtered_refs = Vec::with_capacity(new_refs.len());
        for (note, nr) in &new_refs {
            let cached_note = damus.note_cache_mut().cached_note_or_insert(nr.key, note);

            if ViewFilter::filter_notes(cached_note, note) {
                filtered_refs.push(*nr);
            }
        }

        let timeline = &mut damus.timelines[timeline_ind];

        let prev_items = timeline.notes(ViewFilter::Notes).len();

        timeline.view_mut(ViewFilter::Notes).notes =
            timeline::merge_sorted_vecs(timeline.notes(ViewFilter::Notes), &filtered_refs);

        let new_items = timeline.notes(ViewFilter::Notes).len() - prev_items;

        // TODO: technically items could have been added inbetween
        if new_items > 0 {
            damus.timelines[timeline_ind]
                .view(ViewFilter::Notes)
                .list
                .borrow_mut()
                .items_inserted_at_start(new_items);
        }
    }

    Ok(())
}

#[cfg(feature = "profiling")]
fn setup_profiling() {
    puffin::set_scopes_on(true); // tell puffin to collect data
}

fn setup_initial_nostrdb_subs(damus: &mut Damus) -> Result<()> {
    let timelines = damus.timelines.len();
    for i in 0..timelines {
        let filters: Vec<nostrdb::Filter> = damus.timelines[i]
            .filter
            .iter()
            .map(crate::filter::convert_enostr_filter)
            .collect();
        damus.timelines[i].subscription = Some(damus.ndb.subscribe(filters.clone())?);
        let txn = Transaction::new(&damus.ndb)?;
        debug!(
            "querying sub {} {:?}",
            damus.timelines[i].subscription.as_ref().unwrap().id,
            damus.timelines[i].filter
        );
        let results = damus.ndb.query(
            &txn,
            filters,
            damus.timelines[i].filter[0].limit.unwrap_or(200) as i32,
        )?;

        let filters = {
            let views = &damus.timelines[i].views;
            let filters: Vec<fn(&CachedNote, &Note) -> bool> =
                views.iter().map(|v| v.filter.filter()).collect();
            filters
        };

        for result in results {
            for (j, filter) in filters.iter().enumerate() {
                if filter(
                    damus
                        .note_cache_mut()
                        .cached_note_or_insert_mut(result.note_key, &result.note),
                    &result.note,
                ) {
                    damus.timelines[i].views[j].notes.push(NoteRef {
                        key: result.note_key,
                        created_at: result.note.created_at(),
                    })
                }
            }
        }
    }

    Ok(())
}

fn update_damus(damus: &mut Damus, ctx: &egui::Context) {
    if damus.state == DamusState::Initializing {
        #[cfg(feature = "profiling")]
        setup_profiling();

        damus.pool = RelayPool::new();
        relay_setup(&mut damus.pool, ctx);
        damus.state = DamusState::Initialized;
        setup_initial_nostrdb_subs(damus).expect("home subscription failed");
    }

    if let Err(err) = try_process_event(damus, ctx) {
        error!("error processing event: {}", err);
    }
}

fn process_event(damus: &mut Damus, _subid: &str, event: &str) {
    #[cfg(feature = "profiling")]
    puffin::profile_function!();

    //info!("processing event {}", event);
    if let Err(_err) = damus.ndb.process_event(event) {
        error!("error processing event {}", event);
    }
}

fn get_unknown_ids<'a>(txn: &'a Transaction, damus: &mut Damus) -> Result<Vec<UnknownId<'a>>> {
    #[cfg(feature = "profiling")]
    puffin::profile_function!();

    let mut ids: HashSet<UnknownId> = HashSet::new();
    let mut new_cached_notes: Vec<(NoteKey, CachedNote)> = vec![];

    for timeline in &damus.timelines {
        for noteref in timeline.notes(ViewFilter::NotesAndReplies) {
            let note = damus.ndb.get_note_by_key(txn, noteref.key)?;
            let note_key = note.key().unwrap();
            let cached_note = damus.note_cache().cached_note(noteref.key);
            let cached_note = if let Some(cn) = cached_note {
                cn.clone()
            } else {
                let new_cached_note = CachedNote::new(&note);
                new_cached_notes.push((note_key, new_cached_note.clone()));
                new_cached_note
            };

            let _ = get_unknown_note_ids(
                &damus.ndb,
                &cached_note,
                txn,
                &note,
                note.key().unwrap(),
                &mut ids,
            );
        }
    }

    // This is mainly done to avoid the double mutable borrow that would happen
    // if we tried to update the note_cache mutably in the loop above
    for (note_key, note) in new_cached_notes {
        damus.note_cache_mut().cache_mut().insert(note_key, note);
    }

    Ok(ids.into_iter().collect())
}

fn get_unknown_ids_filter(ids: &[UnknownId<'_>]) -> Option<Vec<Filter>> {
    if ids.is_empty() {
        return None;
    }

    let mut filters: Vec<Filter> = vec![];

    let pks: Vec<Pubkey> = ids
        .iter()
        .flat_map(|id| id.is_pubkey().map(Pubkey::new))
        .collect();
    if !pks.is_empty() {
        let pk_filter = Filter::new().authors(pks).kinds(vec![0]);

        filters.push(pk_filter);
    }

    let note_ids: Vec<enostr::EventId> = ids
        .iter()
        .flat_map(|id| id.is_id().map(|id| enostr::EventId::new(*id)))
        .collect();
    if !note_ids.is_empty() {
        filters.push(Filter::new().ids(note_ids));
    }

    Some(filters)
}

fn handle_eose(damus: &mut Damus, subid: &str, relay_url: &str) -> Result<()> {
    if subid.starts_with("initial") {
        let txn = Transaction::new(&damus.ndb)?;
        let ids = get_unknown_ids(&txn, damus)?;
        if let Some(filters) = get_unknown_ids_filter(&ids) {
            info!("Getting {} unknown ids from {}", ids.len(), relay_url);
            let msg = ClientMessage::req("unknown_ids".to_string(), filters);
            damus.pool.send_to(&msg, relay_url);
        }
    } else if subid == "unknown_ids" {
        let msg = ClientMessage::close("unknown_ids".to_string());
        damus.pool.send_to(&msg, relay_url);
    } else {
        warn!("got unknown eose subid {}", subid);
    }

    Ok(())
}

fn process_message(damus: &mut Damus, relay: &str, msg: &RelayMessage) {
    match msg {
        RelayMessage::Event(subid, ev) => process_event(damus, subid, ev),
        RelayMessage::Notice(msg) => warn!("Notice from {}: {}", relay, msg),
        RelayMessage::OK(cr) => info!("OK {:?}", cr),
        RelayMessage::Eose(sid) => {
            if let Err(err) = handle_eose(damus, sid, relay) {
                error!("error handling eose: {}", err);
            }
        }
    }
}

fn render_damus(damus: &mut Damus, ctx: &Context) {
    if is_mobile() {
        render_damus_mobile(ctx, damus);
    } else {
        render_damus_desktop(ctx, damus);
    }

    ctx.request_repaint_after(Duration::from_secs(1));

    #[cfg(feature = "profiling")]
    puffin_egui::profiler_window(ctx);
}

impl Damus {
    /// Called once before the first frame.
    pub fn new<P: AsRef<Path>>(
        cc: &eframe::CreationContext<'_>,
        data_path: P,
        args: Vec<String>,
    ) -> Self {
        // This is also where you can customized the look at feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        //if let Some(storage) = cc.storage {
        //return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        //}
        //

        setup_cc(cc);

        let mut timelines: Vec<Timeline> = vec![];
        let _initial_limit = 100;
        if args.len() > 1 {
            for arg in &args[1..] {
                let filter = serde_json::from_str(arg).unwrap();
                timelines.push(Timeline::new(filter));
            }
        } else {
            let filter = serde_json::from_str(include_str!("../queries/timeline.json")).unwrap();
            timelines.push(Timeline::new(filter));
        };

        let imgcache_dir = data_path.as_ref().join(ImageCache::rel_datadir());
        let _ = std::fs::create_dir_all(imgcache_dir.clone());

        let mut config = Config::new();
        config.set_ingester_threads(2);
        Self {
            state: DamusState::Initializing,
            pool: RelayPool::new(),
            img_cache: ImageCache::new(imgcache_dir),
            note_cache: NoteCache::default(),
            selected_timeline: 0,
            nav: Vec::with_capacity(6),
            timelines,
            textmode: false,
            ndb: Ndb::new(data_path.as_ref().to_str().expect("db path ok"), &config).expect("ndb"),
            account_manager: AccountManager::new(
                // TODO: should pull this from settings
                None,
                // TODO: use correct KeyStorage mechanism for current OS arch
                crate::key_storage::KeyStorage::None,
            ),
            //compose: "".to_string(),
            frame_history: FrameHistory::default(),
        }
    }

    pub fn mock<P: AsRef<Path>>(data_path: P) -> Self {
        let mut timelines: Vec<Timeline> = vec![];
        let _initial_limit = 100;
        let filter = serde_json::from_str(include_str!("../queries/global.json")).unwrap();
        timelines.push(Timeline::new(filter));

        let imgcache_dir = data_path.as_ref().join(ImageCache::rel_datadir());
        let _ = std::fs::create_dir_all(imgcache_dir.clone());

        let mut config = Config::new();
        config.set_ingester_threads(2);
        Self {
            state: DamusState::Initializing,
            pool: RelayPool::new(),
            img_cache: ImageCache::new(imgcache_dir),
            note_cache: NoteCache::default(),
            selected_timeline: 0,
            timelines,
            nav: vec![],
            textmode: false,
            ndb: Ndb::new(data_path.as_ref().to_str().expect("db path ok"), &config).expect("ndb"),
            account_manager: AccountManager::new(None, crate::key_storage::KeyStorage::None),
            frame_history: FrameHistory::default(),
        }
    }

    pub fn note_cache_mut(&mut self) -> &mut NoteCache {
        &mut self.note_cache
    }

    pub fn note_cache(&self) -> &NoteCache {
        &self.note_cache
    }

    pub fn selected_timeline(&mut self) -> &mut Timeline {
        &mut self.timelines[self.selected_timeline as usize]
    }

    pub fn select_down(&mut self) {
        self.selected_timeline().current_view_mut().select_down();
    }

    pub fn select_up(&mut self) {
        self.selected_timeline().current_view_mut().select_up();
    }

    pub fn select_left(&mut self) {
        if self.selected_timeline - 1 < 0 {
            return;
        }
        self.selected_timeline -= 1;
    }

    pub fn select_right(&mut self) {
        if self.selected_timeline + 1 >= self.timelines.len() as i32 {
            return;
        }
        self.selected_timeline += 1;
    }
}

/*
fn circle_icon(ui: &mut egui::Ui, openness: f32, response: &egui::Response) {
    let stroke = ui.style().interact(&response).fg_stroke;
    let radius = egui::lerp(2.0..=3.0, openness);
    ui.painter()
        .circle_filled(response.rect.center(), radius, stroke.color);
}
*/

fn top_panel(ctx: &egui::Context) -> egui::TopBottomPanel {
    let top_margin = egui::Margin {
        top: 4.0,
        left: 8.0,
        right: 8.0,
        ..Default::default()
    };

    let frame = Frame {
        inner_margin: top_margin,
        fill: ctx.style().visuals.panel_fill,
        ..Default::default()
    };

    egui::TopBottomPanel::top("top_panel")
        .frame(frame)
        .show_separator_line(false)
}

fn render_panel(ctx: &egui::Context, app: &mut Damus, timeline_ind: usize) {
    top_panel(ctx).show(ctx, |ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
            ui.visuals_mut().button_frame = false;

            if let Some(new_visuals) =
                user_requested_visuals_change(is_mobile(), ctx.style().visuals.dark_mode, ui)
            {
                ctx.set_visuals(new_visuals)
            }

            if ui
                .add(egui::Button::new("A").frame(false))
                .on_hover_text("Text mode")
                .clicked()
            {
                app.textmode = !app.textmode;
            }

            /*
            if ui
                .add(egui::Button::new("+").frame(false))
                .on_hover_text("Add Timeline")
                .clicked()
            {
                app.n_panels += 1;
            }

            if app.n_panels != 1
                && ui
                    .add(egui::Button::new("-").frame(false))
                    .on_hover_text("Remove Timeline")
                    .clicked()
            {
                app.n_panels -= 1;
            }
            */

            //#[cfg(feature = "profiling")]
            {
                ui.weak(format!(
                    "FPS: {:.2}, {:10.1}ms",
                    app.frame_history.fps(),
                    app.frame_history.mean_frame_time() * 1e3
                ));

                ui.weak(format!(
                    "{} notes",
                    &app.timelines[timeline_ind]
                        .notes(ViewFilter::NotesAndReplies)
                        .len()
                ));
            }
        });
    });
}

fn render_damus_mobile(ctx: &egui::Context, app: &mut Damus) {
    //render_panel(ctx, app, 0);

    #[cfg(feature = "profiling")]
    puffin::profile_function!();

    main_panel(&ctx.style()).show(ctx, |ui| {
        timeline::timeline_view(ui, app, 0);
    });
}

fn main_panel(style: &Style) -> egui::CentralPanel {
    let inner_margin = egui::Margin {
        top: if crate::ui::is_mobile() { 50.0 } else { 0.0 },
        left: 0.0,
        right: 0.0,
        bottom: 0.0,
    };
    egui::CentralPanel::default().frame(Frame {
        inner_margin,
        fill: style.visuals.panel_fill,
        ..Default::default()
    })
}

fn render_damus_desktop(ctx: &egui::Context, app: &mut Damus) {
    render_panel(ctx, app, 0);
    #[cfg(feature = "profiling")]
    puffin::profile_function!();

    let screen_size = ctx.screen_rect().width();
    let calc_panel_width = (screen_size / app.timelines.len() as f32) - 30.0;
    let min_width = 320.0;
    let need_scroll = calc_panel_width < min_width;
    let panel_sizes = if need_scroll {
        Size::exact(min_width)
    } else {
        Size::remainder()
    };

    main_panel(&ctx.style()).show(ctx, |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        if need_scroll {
            egui::ScrollArea::horizontal().show(ui, |ui| {
                timelines_view(ui, panel_sizes, app, app.timelines.len());
            });
        } else {
            timelines_view(ui, panel_sizes, app, app.timelines.len());
        }
    });
}

fn timelines_view(ui: &mut egui::Ui, sizes: Size, app: &mut Damus, timelines: usize) {
    StripBuilder::new(ui)
        .size(Size::exact(40.0))
        .sizes(sizes, timelines)
        .clip(true)
        .horizontal(|mut strip| {
            strip.cell(|ui| {
                if DesktopSidePanel::new(
                    app.account_manager
                        .get_selected_account()
                        .map(|a| a.pubkey.bytes()),
                    SimpleProfilePreviewController::new(&app.ndb, &mut app.img_cache),
                )
                .show(ui)
                .clicked()
                {
                    // clicked pfp
                }
            });

            for timeline_ind in 0..timelines {
                strip.cell(|ui| timeline::timeline_view(ui, app, timeline_ind));
            }
        });
}

/*
fn postbox(ui: &mut egui::Ui, app: &mut Damus) {
    let _output = egui::TextEdit::multiline(&mut app.compose)
        .hint_text("Type something!")
        .show(ui);

    let width = ui.available_width();
    let height = 100.0;
    let shapes = [Shape::Rect(RectShape {
        rect: epaint::Rect::from_min_max(pos2(10.0, 10.0), pos2(width, height)),
        rounding: epaint::Rounding::same(10.0),
        fill: Color32::from_rgb(0x25, 0x25, 0x25),
        stroke: Stroke::new(2.0, Color32::from_rgb(0x39, 0x39, 0x39)),
    })];

    ui.painter().extend(shapes);
}
    */

impl eframe::App for Damus {
    /// Called by the frame work to save state before shutdown.
    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        //eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    /// Put your widgets into a `SidePanel`, `TopPanel`, `CentralPanel`, `Window` or `Area`.
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.frame_history
            .on_new_frame(ctx.input(|i| i.time), frame.info().cpu_usage);

        #[cfg(feature = "profiling")]
        puffin::GlobalProfiler::lock().new_frame();
        update_damus(self, ctx);
        render_damus(self, ctx);
    }
}
