//! Derived data computed off the interface thread: drawings of views and sheets, the 3D scene, the ΝΟΚ report and
//! schedules (docs/architecture.md §3).
//!
//! A [`Derived`] owns a small pool of worker threads. The interface calls [`Derived::request`] every frame with
//! the current document and its revision ([`crate::Session::revision`]); the call never blocks and does nothing
//! when that revision is already computed or being computed. [`Derived::latest`] returns the newest finished
//! result, which may be for an older revision while a newer one is computed: the last good result stays on
//! screen, never a blank view.
//!
//! Rules:
//! - Each request (e.g. "plan view #12 in Greek") has at most one job running. Newer requests that arrive while it
//!   runs replace each other; when the job ends, the newest one runs next. Intermediate revisions are skipped.
//! - A finished result replaces the stored one only if its revision is newer; older results are discarded.
//! - A revision lower than the stored one means a new session (a document was opened): the request's stored
//!   results are dropped, and jobs still running for the old session are discarded when they finish.
//! - Errors are kept per request ([`Derived::error`]) next to the last good result.
//!
//! ```no_run
//! use kanavos_engine::derived::{Derived, Request};
//! # let session = kanavos_engine::Session::new(kanavos_model::starter());
//! # let view = kanavos_model::Id(1);
//! let derived = Derived::new(); // keep it in the application state
//! let req = Request::View { view, locale: kanavos_units::Locale::El };
//! // Every frame:
//! derived.request(session.document(), session.revision(), req.clone());
//! if let Some((revision, drawing)) = derived.latest_drawing(&req) {
//!     let updating = revision != session.revision() || derived.is_updating(&req);
//!     // draw `drawing`, with a subtle "updating" mark when `updating`
//! }
//! ```

use std::collections::{BTreeSet, HashMap};
use std::panic::AssertUnwindSafe;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use kanavos_drawing::Drawing;
use kanavos_drawing::table::Table;
use kanavos_model::{Category, Document, Id, ScheduleKind};
use kanavos_rules_gr::NokReport;
use kanavos_units::Locale;
use kanavos_views::{Scene, SceneOptions};

/// What to compute.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Request {
    /// A 2D view: plan, section, elevation (model metres), site and regulation views, schedules and area tables
    /// (paper millimetres), as drawn on sheets.
    View {
        /// View.
        view: Id,
        /// Language of generated text.
        locale: Locale,
    },
    /// A whole sheet in paper millimetres: border, title block and viewports.
    Sheet {
        /// Sheet.
        sheet: Id,
        /// Language of generated text.
        locale: Locale,
    },
    /// The 3D scene.
    Scene {
        /// Only these storeys; `None` for all.
        storeys: Option<BTreeSet<Id>>,
        /// Categories left out.
        hidden: BTreeSet<Category>,
    },
    /// The ΝΟΚ report (coverage, floor area, height, per-storey areas).
    Nok,
    /// A schedule table.
    Schedule {
        /// Which schedule.
        kind: ScheduleKind,
        /// Language of headers.
        locale: Locale,
    },
    /// The area table (πίνακας εμβαδών).
    AreaTable {
        /// Language of headers.
        locale: Locale,
    },
}

impl Request {
    /// The whole 3D scene.
    pub fn scene() -> Request {
        Request::Scene { storeys: None, hidden: BTreeSet::new() }
    }
}

/// A computed result. Cloning is cheap (shared).
#[derive(Debug, Clone)]
pub enum Product {
    /// A drawing (views and sheets).
    Drawing(Arc<Drawing>),
    /// The 3D scene.
    Scene(Arc<Scene>),
    /// The ΝΟΚ report.
    Nok(Arc<NokReport>),
    /// A table (schedules, area table).
    Table(Arc<Table>),
}

/// Computes a request synchronously, on the calling thread. [`Derived`] runs the same function on its workers.
pub fn compute(doc: &Document, request: &Request) -> Result<Product, String> {
    match request {
        Request::View { view, locale } => kanavos_sheets::render_view(doc, *view, *locale)
            .map(|d| Product::Drawing(Arc::new(d)))
            .map_err(|e| e.to_string()),
        Request::Sheet { sheet, locale } => kanavos_sheets::sheet_drawing(doc, *sheet, *locale)
            .map(|d| Product::Drawing(Arc::new(d)))
            .map_err(|e| e.to_string()),
        Request::Scene { storeys, hidden } => {
            let opts = SceneOptions { storeys: storeys.clone(), hidden: hidden.clone() };
            Ok(Product::Scene(Arc::new(kanavos_views::scene(doc, &opts))))
        }
        Request::Nok => Ok(Product::Nok(Arc::new(kanavos_rules_gr::nok_report(doc)))),
        Request::Schedule { kind, locale } => {
            Ok(Product::Table(Arc::new(kanavos_schedules::schedule(doc, *kind, *locale))))
        }
        Request::AreaTable { locale } => {
            Ok(Product::Table(Arc::new(kanavos_rules_gr::area_table(doc, *locale))))
        }
    }
}

#[derive(Default)]
struct Entry {
    /// Newest finished result.
    ready: Option<(u64, Product)>,
    /// Newest error, if it is newer than `ready`.
    error: Option<(u64, String)>,
    /// Newest requested revision not started yet.
    pending: Option<(u64, Document)>,
    /// Revision being computed.
    running: Option<u64>,
    /// A worker owns this request (queued or computing).
    active: bool,
    /// Bumped when the request's results are dropped, so jobs from before are discarded.
    generation: u64,
}

impl Entry {
    fn newest_known(&self) -> Option<u64> {
        let r = self.ready.as_ref().map(|r| r.0);
        let e = self.error.as_ref().map(|e| e.0);
        let p = self.pending.as_ref().map(|p| p.0);
        [r, e, p, self.running].into_iter().flatten().max()
    }
}

type Notifier = Box<dyn Fn() + Send + Sync>;

struct Shared {
    entries: Mutex<HashMap<Request, Entry>>,
    idle: Condvar,
    notify: Option<Notifier>,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, HashMap<Request, Entry>> {
        // A worker that panicked while holding the lock leaves consistent data (entries are updated whole).
        self.entries.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

enum Msg {
    Job(Request),
    Stop,
}

/// Background computation of derived data, keyed by request and document revision.
pub struct Derived {
    shared: Arc<Shared>,
    tx: Sender<Msg>,
    workers: Vec<JoinHandle<()>>,
}

impl std::fmt::Debug for Derived {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Derived")
            .field("workers", &self.workers.len())
            .field("busy", &self.is_busy())
            .finish()
    }
}

impl Default for Derived {
    fn default() -> Self {
        Derived::new()
    }
}

fn default_workers() -> usize {
    std::thread::available_parallelism().map(|n| n.get().saturating_sub(1)).unwrap_or(1).clamp(1, 4)
}

impl Derived {
    /// A pool with one worker per spare CPU core (1 to 4).
    pub fn new() -> Derived {
        Derived::build(default_workers(), None)
    }

    /// A pool with a given number of workers (at least one).
    pub fn with_workers(workers: usize) -> Derived {
        Derived::build(workers.max(1), None)
    }

    /// A pool that calls `notify` (from a worker thread) whenever a result or error arrives, e.g. to ask the
    /// interface for a repaint.
    pub fn with_notifier(workers: usize, notify: impl Fn() + Send + Sync + 'static) -> Derived {
        Derived::build(workers.max(1), Some(Box::new(notify)))
    }

    fn build(workers: usize, notify: Option<Notifier>) -> Derived {
        let shared = Arc::new(Shared { entries: Mutex::new(HashMap::new()), idle: Condvar::new(), notify });
        let (tx, rx) = channel::<Msg>();
        let rx = Arc::new(Mutex::new(rx));
        let mut handles = Vec::with_capacity(workers);
        for i in 0..workers {
            let shared = Arc::clone(&shared);
            let rx = Arc::clone(&rx);
            let spawned = std::thread::Builder::new()
                .name(format!("kanavos-derived-{i}"))
                .spawn(move || worker(&shared, &rx));
            if let Ok(h) = spawned {
                handles.push(h);
            }
        }
        Derived { shared, tx, workers: handles }
    }

    /// Asks for `request` on `doc` at `revision`. Never blocks: when the revision is new, a copy of the document
    /// (cheap: shared storage) is handed to a worker. Without workers (threads could not be started) the result
    /// is computed here.
    pub fn request(&self, doc: &Document, revision: u64, request: Request) {
        let send = {
            let mut entries = self.shared.lock();
            let e = entries.entry(request.clone()).or_default();
            if e.newest_known().is_some_and(|newest| revision < newest) {
                // A new session: forget this request's results.
                e.ready = None;
                e.error = None;
                e.pending = None;
                e.generation += 1;
            }
            let have = e.ready.as_ref().is_some_and(|r| r.0 == revision)
                || e.error.as_ref().is_some_and(|x| x.0 == revision)
                || e.running == Some(revision)
                || e.pending.as_ref().is_some_and(|p| p.0 == revision);
            if have {
                return;
            }
            e.pending = Some((revision, doc.clone()));
            if e.active {
                false
            } else {
                e.active = true;
                true
            }
        };
        if send && (self.workers.is_empty() || self.tx.send(Msg::Job(request.clone())).is_err()) {
            run_job(&self.shared, &request);
        }
    }

    /// The newest finished result of a request, with the revision it was computed for.
    pub fn latest(&self, request: &Request) -> Option<(u64, Product)> {
        self.shared.lock().get(request).and_then(|e| e.ready.clone())
    }

    /// The newest drawing of a view or sheet request.
    pub fn latest_drawing(&self, request: &Request) -> Option<(u64, Arc<Drawing>)> {
        match self.latest(request)? {
            (rev, Product::Drawing(d)) => Some((rev, d)),
            _ => None,
        }
    }

    /// The newest 3D scene of a scene request.
    pub fn latest_scene(&self, request: &Request) -> Option<(u64, Arc<Scene>)> {
        match self.latest(request)? {
            (rev, Product::Scene(s)) => Some((rev, s)),
            _ => None,
        }
    }

    /// The newest ΝΟΚ report.
    pub fn latest_nok(&self) -> Option<(u64, Arc<NokReport>)> {
        match self.latest(&Request::Nok)? {
            (rev, Product::Nok(r)) => Some((rev, r)),
            _ => None,
        }
    }

    /// The newest table of a schedule or area-table request.
    pub fn latest_table(&self, request: &Request) -> Option<(u64, Arc<Table>)> {
        match self.latest(request)? {
            (rev, Product::Table(t)) => Some((rev, t)),
            _ => None,
        }
    }

    /// The newest error of a request, if it is newer than its last good result.
    pub fn error(&self, request: &Request) -> Option<(u64, String)> {
        self.shared.lock().get(request).and_then(|e| e.error.clone())
    }

    /// Whether a request is queued or being computed.
    pub fn is_updating(&self, request: &Request) -> bool {
        self.shared.lock().get(request).is_some_and(|e| e.active)
    }

    /// Whether any request is queued or being computed (for a status-bar indicator).
    pub fn is_busy(&self) -> bool {
        self.shared.lock().values().any(|e| e.active)
    }

    /// Waits until nothing is queued or running, at most `timeout`. Returns whether the pool is idle. For tools
    /// and tests; the interface never waits.
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut entries = self.shared.lock();
        loop {
            if !entries.values().any(|e| e.active) {
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            entries = match self.shared.idle.wait_timeout(entries, deadline - now) {
                Ok((g, _)) => g,
                Err(p) => p.into_inner().0,
            };
        }
    }

    /// Requests and waits for one result (for tools, the control protocol and tests).
    pub fn get(
        &self,
        doc: &Document,
        revision: u64,
        request: Request,
        timeout: Duration,
    ) -> Result<Product, String> {
        self.request(doc, revision, request.clone());
        self.wait_idle(timeout);
        let entries = self.shared.lock();
        let e = entries.get(&request).ok_or_else(|| "not computed".to_owned())?;
        match (&e.ready, &e.error) {
            (_, Some((rev, msg))) if *rev == revision => Err(msg.clone()),
            (Some((rev, p)), _) if *rev == revision => Ok(p.clone()),
            _ => Err("timed out".to_owned()),
        }
    }

    /// Drops a request's results (e.g. when its view is closed).
    pub fn forget(&self, request: &Request) {
        let mut entries = self.shared.lock();
        if let Some(e) = entries.get_mut(request) {
            e.ready = None;
            e.error = None;
            e.pending = None;
            e.generation += 1;
            if !e.active {
                entries.remove(request);
            }
        }
    }

    /// Drops every result (e.g. when another document is opened).
    pub fn clear(&self) {
        let mut entries = self.shared.lock();
        entries.retain(|_, e| e.active);
        for e in entries.values_mut() {
            e.ready = None;
            e.error = None;
            e.pending = None;
            e.generation += 1;
        }
    }

    /// Number of worker threads.
    pub fn workers(&self) -> usize {
        self.workers.len()
    }
}

impl Drop for Derived {
    fn drop(&mut self) {
        for _ in &self.workers {
            let _ = self.tx.send(Msg::Stop);
        }
        for h in self.workers.drain(..) {
            let _ = h.join();
        }
    }
}

fn worker(shared: &Shared, rx: &Mutex<Receiver<Msg>>) {
    loop {
        let msg = {
            let guard = rx.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.recv()
        };
        match msg {
            Ok(Msg::Job(request)) => run_job(shared, &request),
            Ok(Msg::Stop) | Err(_) => break,
        }
    }
}

/// Computes a request until no newer revision is pending, then releases it.
fn run_job(shared: &Shared, request: &Request) {
    loop {
        let (revision, doc, generation) = {
            let mut entries = shared.lock();
            let Some(e) = entries.get_mut(request) else { return };
            match e.pending.take() {
                Some((rev, doc)) => {
                    e.running = Some(rev);
                    (rev, doc, e.generation)
                }
                None => {
                    e.active = false;
                    e.running = None;
                    shared.idle.notify_all();
                    return;
                }
            }
        };
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| compute(&doc, request)))
            .unwrap_or_else(|_| Err("internal error while computing".to_owned()));
        {
            let mut entries = shared.lock();
            let Some(e) = entries.get_mut(request) else { return };
            e.running = None;
            if e.generation == generation {
                match result {
                    Ok(p) => {
                        if e.ready.as_ref().is_none_or(|r| r.0 < revision) {
                            e.ready = Some((revision, p));
                        }
                        if e.error.as_ref().is_some_and(|x| x.0 <= revision) {
                            e.error = None;
                        }
                    }
                    Err(msg) => {
                        if e.ready.as_ref().is_none_or(|r| r.0 < revision) {
                            e.error = Some((revision, msg));
                        }
                    }
                }
            }
        }
        if let Some(n) = &shared.notify {
            n();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use crate::commands::CreateElement;
    use kanavos_geom::Point2;
    use kanavos_model::{ElementKind, ReferenceLine, Structure, ViewKind, Wall, starter};
    use kanavos_units::Length;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const WAIT: Duration = Duration::from_secs(30);

    fn plan_request(doc: &Document) -> Request {
        let v = doc.views().find(|v| matches!(v.kind, ViewKind::Plan { .. })).unwrap().id;
        Request::View { view: v, locale: Locale::El }
    }

    fn add_wall(s: &mut Session, x: f64) {
        let st = s.document().storeys()[0].id;
        let bu = s.document().defaults().exterior_wall.unwrap();
        s.run(CreateElement::on(
            st,
            ElementKind::Wall(Wall {
                start: Point2::new(x, 0.0),
                end: Point2::new(x + 4.0, 0.0),
                bulge: 0.0,
                reference: ReferenceLine::Centre,
                structure: Structure::Layered { build_up: bu },
                height: Length::m(3.0),
                base_offset: Length::ZERO,
            }),
        ))
        .unwrap();
    }

    #[test]
    fn results_arrive_for_the_requested_revision() {
        let calls = Arc::new(AtomicUsize::new(0));
        let c2 = Arc::clone(&calls);
        let d = Derived::with_notifier(2, move || {
            c2.fetch_add(1, Ordering::SeqCst);
        });
        let mut s = Session::new(starter());
        add_wall(&mut s, 0.0);
        let req = plan_request(s.document());
        d.request(s.document(), s.revision(), req.clone());
        assert!(d.wait_idle(WAIT));
        let (rev, drawing) = d.latest_drawing(&req).unwrap();
        assert_eq!(rev, s.revision());
        assert!(!drawing.items.is_empty());
        assert!(calls.load(Ordering::SeqCst) >= 1);
        // Asking again for the same revision does nothing.
        d.request(s.document(), s.revision(), req.clone());
        assert!(!d.is_updating(&req) || d.wait_idle(WAIT));
    }

    #[test]
    fn last_good_result_stays_while_recomputing_and_old_results_are_discarded() {
        let d = Derived::with_workers(1);
        let mut s = Session::new(starter());
        let req = Request::Nok;
        d.request(s.document(), s.revision(), req.clone());
        assert!(d.wait_idle(WAIT));
        let first = d.latest_nok().unwrap().0;
        for i in 0..5 {
            add_wall(&mut s, f64::from(i) * 5.0);
            d.request(s.document(), s.revision(), req.clone());
            // Always something to show.
            assert!(d.latest_nok().is_some());
        }
        assert!(d.wait_idle(WAIT));
        let (rev, _) = d.latest_nok().unwrap();
        assert_eq!(rev, s.revision());
        assert!(rev > first);
    }

    #[test]
    fn a_new_session_drops_old_results() {
        let d = Derived::with_workers(2);
        let mut s = Session::new(starter());
        for i in 0..3 {
            add_wall(&mut s, f64::from(i) * 5.0);
        }
        let req = Request::scene();
        d.request(s.document(), s.revision(), req.clone());
        assert!(d.wait_idle(WAIT));
        assert_eq!(d.latest_scene(&req).unwrap().0, 3);
        let fresh = Session::new(starter());
        d.request(fresh.document(), fresh.revision(), req.clone());
        assert!(d.wait_idle(WAIT));
        let (rev, scene) = d.latest_scene(&req).unwrap();
        assert_eq!(rev, 0);
        assert!(scene.items.is_empty());
    }

    #[test]
    fn errors_are_reported_per_request() {
        let d = Derived::with_workers(1);
        let doc = starter();
        let req = Request::View { view: Id(999_999), locale: Locale::En };
        assert!(d.get(&doc, 0, req.clone(), WAIT).is_err());
        assert!(d.error(&req).is_some());
        assert!(d.latest(&req).is_none());
        let ok = d.get(&doc, 0, Request::AreaTable { locale: Locale::El }, WAIT);
        assert!(matches!(ok, Ok(Product::Table(_))));
        d.clear();
        assert!(d.latest(&Request::AreaTable { locale: Locale::El }).is_none());
    }
}
