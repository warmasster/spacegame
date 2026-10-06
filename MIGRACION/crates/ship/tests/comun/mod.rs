//! What the logic tests (`logica_*.rs`) share: every ship of the library loaded once per test
//! binary, a ship on a bench (`Rig`: its structure, its systems, the world round it) worked as a
//! hand works it, and what the tests read off any ship without knowing which it is — who owns each
//! port, which controls a hand can work, what feeds what on a network. Nothing here names a ship,
//! a panel or a signal of one: a new ship gets every test for free.
#![allow(dead_code)]
pub mod vuelo;
use glam::{DQuat, DVec3, Vec3};
use lunar_controls::{Intent, Mods, Outcome};
use lunar_core::structure::{Library, state::Structure};
use lunar_machines::{EdgeKind, PortIo, net::Medium};
use lunar_ship::{Ship, ShipKind, ShipLibrary, World, ship::TICK};
use lunar_signals::{SignalId, Writer};
use std::{
    path::Path,
    sync::{Arc, OnceLock},
};

pub struct Fleet {
    pub lib: Library,
    pub kinds: Vec<Arc<ShipKind>>,
}

/// Every ship of `assets/defs/ships`, assembled once for all the tests of a file.
pub fn fleet() -> &'static Fleet {
    static FLEET: OnceLock<Fleet> = OnceLock::new();
    FLEET.get_or_init(|| {
        let defs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs");
        let mut lib = Library::load(&defs.join("structures")).unwrap_or_else(|e| panic!("{e}"));
        let (ships, bps) = ShipLibrary::load(&defs, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
        lib.blueprints.extend(bps);
        assert!(!ships.kinds.is_empty(), "no hay naves en assets/defs/ships");
        Fleet { lib, kinds: ships.kinds.clone() }
    })
}

/// A ship standing on the Moon with its weight on its gear.
pub fn grounded() -> World {
    World { gravity: Vec3::new(0.0, -1.62, 0.0), altitude: 0.0, ..World::default() }
}

/// The same ship well clear of the ground (what its gear and its interlocks see in flight).
pub fn airborne() -> World {
    World { altitude: 400.0, ..grounded() }
}

/// One ship on the bench.
pub struct Rig {
    pub s: Structure,
    pub ship: Ship,
    pub kind: Arc<ShipKind>,
    pub w: World,
    /// What its machines say they hold, to keep within bounds: as a share (0..1), or by weight.
    vitals: Vec<(SignalId, bool)>,
}

impl Fleet {
    /// A fresh ship of `kind`, as the game puts it down (seed 7, like every other test).
    pub fn rig(&self, kind: &Arc<ShipKind>) -> Rig {
        let bp = self.lib.blueprint(&kind.blueprint).unwrap_or_else(|| panic!("{}: no hay plano {}", kind.id, kind.blueprint));
        let mut s = Structure::new(1, bp, &self.lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
        let mut ship = Ship::new(kind.clone(), 1, 7).unwrap_or_else(|e| panic!("{}: {e}", kind.id));
        let w = grounded();
        ship.update(&mut s, &w, 0.0);
        let mut vitals = Vec::new();
        for (m, plan) in kind.machines.iter().enumerate() {
            for (field, share) in [("soc", true), ("nivel", true), ("carga", true), ("masa", false)] {
                if let Some(sig) = ship.store.find(&format!("{}.{field}", plan.id)).filter(|s| ship.store.meta(*s).writer == Writer::Machine(m as u32)) {
                    vitals.push((sig, share));
                }
            }
        }
        Rig { s, ship, kind: kind.clone(), w, vitals }
    }
}

/// Who owns a port of a ship: a machine, an actuator or a panel (their index in the kind), and
/// the role the port has in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Owner {
    Machine(usize, &'static str),
    Actuator(usize, &'static str),
    PanelPower(usize),
    PanelData(usize),
}

impl Rig {
    pub fn id(&self) -> &str {
        &self.kind.id
    }

    pub fn tick(&mut self) {
        self.ship.update(&mut self.s, &self.w, TICK);
    }

    pub fn ticks(&mut self, n: usize) {
        for _ in 0..n {
            self.ship.update(&mut self.s, &self.w, TICK);
        }
    }

    pub fn run(&mut self, secs: f64) {
        self.ticks((secs / TICK).round() as usize);
    }

    /// Run until `done` says so (looked at every tick), `secs` at most: whether it came true.
    /// Nothing here asserts how long anything takes: `secs` is only what bounds a test that fails.
    pub fn until(&mut self, secs: f64, mut done: impl FnMut(&Rig) -> bool) -> bool {
        for _ in 0..(secs / TICK).round() as usize {
            if done(self) {
                return true;
            }
            self.tick();
        }
        done(self)
    }

    pub fn find(&self, name: &str) -> Option<SignalId> {
        self.ship.store.find(name)
    }

    pub fn sig(&self, name: &str) -> f64 {
        self.ship.signal(name).unwrap_or_else(|| panic!("{}: no hay señal '{name}'", self.id()))
    }

    pub fn get(&self, id: SignalId) -> f64 {
        self.ship.store.get(id)
    }

    pub fn name(&self, id: SignalId) -> &str {
        self.ship.store.name(id)
    }

    // ---- controls, as a hand works them ----

    pub fn ctl(&self, id: &str) -> usize {
        self.ship.panels.controls.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("{}: no hay mando '{id}'", self.id()))
    }

    pub fn ctl_kind(&self, k: usize) -> &'static str {
        self.ship.panels.controls[k].mech.kind()
    }

    pub fn value(&self, k: usize) -> f64 {
        let c = &self.ship.panels.controls[k];
        c.mech.value(&c.st)
    }

    pub fn intent(&mut self, k: usize, i: &Intent) -> Outcome {
        let kind = self.kind.clone();
        self.ship.panels.intent(k, i, &self.s, &kind, &self.ship.store)
    }

    /// A click: pressed, held a moment, let go.
    pub fn click(&mut self, k: usize) -> Outcome {
        let o = self.intent(k, &Intent::Press { elem: 0 });
        self.ticks(3);
        self.intent(k, &Intent::Release);
        self.ticks(2);
        o
    }

    /// The control put straight at a value (what a seat's key or another player's copy does).
    pub fn set(&mut self, k: usize, value: f64) -> bool {
        self.intent(k, &Intent::Set { value }).changed
    }

    pub fn turn(&mut self, k: usize, notches: f32) -> Outcome {
        self.intent(k, &Intent::Turn { notches, rate: 4.0, m: Mods::default() })
    }

    /// The cover over control `k` lifted (if it has one and it is down).
    pub fn uncover(&mut self, k: usize) {
        if let Some(cv) = self.ship.panels.controls[k].cover
            && self.value(cv) < 0.5
        {
            self.intent(cv, &Intent::Press { elem: 0 });
            self.intent(cv, &Intent::Release);
        }
    }

    /// The panel control `k` is on.
    pub fn panel_of(&self, k: usize) -> &str {
        &self.kind.panels[self.ship.panels.controls[k].panel].id
    }

    // ---- ports and networks ----

    /// The owner of every port, in the ship's own order (machines, actuators, panels).
    pub fn owners(&self) -> Vec<Owner> {
        let mut out = Vec::with_capacity(self.ship.ports.len());
        for (m, rt) in self.ship.machines.iter().enumerate() {
            assert_eq!(rt.ports.start, out.len(), "{}: los puertos de las máquinas no van seguidos", self.id());
            out.extend(rt.m.ports().iter().map(|p| Owner::Machine(m, p.role)));
        }
        for (a, act) in self.ship.actuators.iter().enumerate() {
            out.extend(act.ports().iter().map(|p| Owner::Actuator(a, p.role)));
        }
        let rest = out.len();
        out.resize(self.ship.ports.len(), Owner::PanelPower(usize::MAX));
        for (p, rt) in self.ship.panels.panels.iter().enumerate() {
            if let Some(i) = rt.power {
                out[i] = Owner::PanelPower(p);
            }
            if let Some(i) = rt.data {
                out[i] = Owner::PanelData(p);
            }
        }
        assert!(out[rest..].iter().all(|o| *o != Owner::PanelPower(usize::MAX)), "{}: hay puertos sin dueño", self.id());
        out
    }

    /// What a port's owner is called, for a failure message a person can act on.
    pub fn owner_name(&self, o: &Owner) -> String {
        match o {
            Owner::Machine(m, role) => format!("máquina {} (puerto '{role}')", self.kind.machines[*m].id),
            Owner::Actuator(a, role) => format!("actuador {} (puerto '{role}')", self.kind.actuators[*a].id),
            Owner::PanelPower(p) => format!("panel {} (energía)", self.kind.panels[*p].id),
            Owner::PanelData(p) => format!("panel {} (datos)", self.kind.panels[*p].id),
        }
    }

    pub fn port_name(&self, k: usize) -> String {
        let o = &self.owners()[k];
        let p = &self.ship.ports[k];
        match self.kind.nets.get(usize::from(p.net)) {
            Some(n) => format!("{} en {}:{}", self.owner_name(o), n.name, self.node_name(usize::from(p.net), p.node)),
            None => format!("{} sin red", self.owner_name(o)),
        }
    }

    /// A node's name: its own if the data named it, else the named junction nearest along the
    /// network's fixed runs (a device's own end of its stub has no name).
    pub fn node_name(&self, net: usize, node: u32) -> String {
        let plan = &self.kind.nets[net];
        if let Some((name, _)) = plan.names.iter().find(|(_, n)| **n == node) {
            return name.clone();
        }
        // breadth first along conduits and fixed joints (not through switches)
        let next = neighbours(plan, &|e| plan.edges[e].kind != EdgeKind::Switch);
        let mut seen = vec![false; plan.nodes.len()];
        let mut queue = std::collections::VecDeque::from([node]);
        seen[node as usize] = true;
        while let Some(v) = queue.pop_front() {
            if let Some((name, _)) = plan.names.iter().find(|(_, n)| **n == v) {
                return format!("{name}~");
            }
            for &other in &next[v as usize] {
                if !std::mem::replace(&mut seen[other as usize], true) {
                    queue.push_back(other);
                }
            }
        }
        format!("#{node}")
    }

    /// The networks of a medium (their indices).
    pub fn nets_of(&self, medium: Medium) -> Vec<usize> {
        (0..self.kind.nets.len()).filter(|&n| self.kind.nets[n].medium == medium).collect()
    }

    /// The control that works a signal (the first of them whose mechanism writes it), if a hand
    /// can: `None` for what only the ship's own logic writes.
    pub fn control_of(&self, sig: SignalId) -> Option<usize> {
        self.ship.panels.controls.iter().position(|c| c.sig == sig || c.sig2 == Some(sig))
    }

    /// The controls that are switches of a network (a breaker, a contactor, a valve): (control,
    /// net, edge), by the signal their edge closes with.
    pub fn switch_controls(&self) -> Vec<(usize, usize, usize)> {
        let mut out = Vec::new();
        for (n, plan) in self.kind.nets.iter().enumerate() {
            for (e, edge) in plan.edges.iter().enumerate() {
                let Some(sig) = edge.signal.as_ref().and_then(|s| self.find(s)) else { continue };
                if let Some(k) = self.control_of(sig) {
                    out.push((k, n, e));
                }
            }
        }
        out
    }

    /// Ports of network `net` with their place in the ship's list.
    pub fn ports_on(&self, net: usize) -> impl Iterator<Item = (usize, &PortIo)> {
        self.ship.ports.iter().enumerate().filter(move |(_, p)| usize::from(p.net) == net)
    }

    /// The nodes of `net` reached from `from` through what conducts when the edges `open` are
    /// open and every other switch is as `closed` says (conduits and fixed joints always do).
    pub fn reach(&self, net: usize, from: &[u32], closed: &dyn Fn(usize) -> bool) -> Vec<bool> {
        let plan = &self.kind.nets[net];
        let next = neighbours(plan, &|e| plan.edges[e].kind != EdgeKind::Switch || closed(e));
        let mut seen = vec![false; plan.nodes.len()];
        let mut stack: Vec<u32> = from.to_vec();
        while let Some(v) = stack.pop() {
            if std::mem::replace(&mut seen[v as usize], true) {
                continue;
            }
            stack.extend(next[v as usize].iter().filter(|o| !seen[**o as usize]));
        }
        seen
    }

    /// Who writes a signal, in words.
    pub fn writer(&self, sig: SignalId) -> String {
        match &self.ship.store.meta(sig).writer {
            Writer::None => "nadie".into(),
            Writer::Control(k) => format!("el mando {}", self.ship.panels.controls[*k as usize].id),
            Writer::Machine(m) => format!("la máquina {}", self.kind.machines[*m as usize].id),
            Writer::Actuator(a) => format!("el actuador {}", self.kind.actuators[*a as usize].id),
            Writer::Derived(_) => "una derivada".into(),
            Writer::World => "la nave".into(),
        }
    }
}

/// Per node of a network, the nodes its edges join it to, of the edges `through` lets by.
pub fn neighbours(plan: &lunar_ship::kind::NetPlan, through: &dyn Fn(usize) -> bool) -> Vec<Vec<u32>> {
    let mut next = vec![Vec::new(); plan.nodes.len()];
    for (k, e) in plan.edges.iter().enumerate() {
        if through(k) {
            next[e.a as usize].push(e.b);
            next[e.b as usize].push(e.a);
        }
    }
    next
}

/// A small generator of its own for the fuzz (SplitMix64): the same seed, the same run, on any
/// machine.
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    /// 0..n.
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    /// 0..1.
    pub fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Fail with every finding, one per line, each saying which ship and what.
pub fn report(what: &str, bad: &[String]) {
    assert!(bad.is_empty(), "{what}: {} hallazgos\n  {}", bad.len(), bad.join("\n  "));
}

// ---------------------------------------------------------------- what a port is for

/// The roles of the electrical ports that give power (a battery's terminals, the output of a
/// generator, of a panel, of a converter): every other electrical port takes it. The one table
/// the logic tests keep by hand; `logica_topologia` holds it to what the ports are seen to do, so a
/// model with a new kind of output fails there until its role is added here.
pub const ELECTRIC_SOURCES: [&str; 2] = ["bornes", "salida"];

/// The orders that set a machine going, by the role its model asks them with.
pub const ON_ROLES: [&str; 3] = ["marcha", "encender", "conectar"];

/// What each port was seen to do over a stretch: offer (produce, or keep a store) and ask.
#[derive(Clone, Debug, Default)]
pub struct Seen {
    pub gives: Vec<bool>,
    pub takes: Vec<bool>,
}

impl Rig {
    /// Run `secs`, noting what every port offers and asks.
    pub fn observe(&mut self, secs: f64, seen: &mut Seen) {
        seen.gives.resize(self.ship.ports.len(), false);
        seen.takes.resize(self.ship.ports.len(), false);
        for _ in 0..(secs / TICK).round() as usize {
            self.tick();
            for (k, p) in self.ship.ports.iter().enumerate() {
                seen.gives[k] |= p.produce > 0.0 || p.store_out > 0.0 || p.store_in > 0.0;
                seen.takes[k] |= p.demand > 0.0;
            }
        }
    }

    /// The signal an order of a machine reads: the one its data names for `role`, else its own.
    pub fn order(&self, machine: usize, role: &str) -> Option<SignalId> {
        let m = &self.kind.machines[machine];
        match m.def.ordenes.get(role).and_then(|v| v.as_str()) {
            Some(name) => self.find(name),
            None => self.find(&format!("{}.{role}", m.id)),
        }
    }

    /// Every machine `which` picks set going by its own switch on a panel (its cover lifted
    /// first), where a hand has one: what was switched, as `machine ← control`.
    pub fn switch_on(&mut self, which: &dyn Fn(&Rig, usize) -> bool) -> Vec<String> {
        let mut done = Vec::new();
        for m in 0..self.kind.machines.len() {
            if !which(self, m) {
                continue;
            }
            for role in ON_ROLES {
                let Some(k) = self.order(m, role).and_then(|s| self.control_of(s)) else { continue };
                if self.ctl_kind(k) != "interruptor" || self.value(k) >= 0.5 {
                    continue;
                }
                self.uncover(k);
                if self.set(k, 1.0) {
                    done.push(format!("{} ← {}", self.kind.machines[m].id, self.ship.panels.controls[k].id));
                }
            }
        }
        done
    }

    /// Whether machine `m` has an electrical port that gives power.
    pub fn is_source(&self, m: usize) -> bool {
        let rt = &self.ship.machines[m];
        rt.m.ports().iter().zip(rt.ports.clone()).any(|(spec, k)| spec.medium == Medium::Electrico && ELECTRIC_SOURCES.contains(&spec.role) && self.ship.ports[k].net != u16::MAX)
    }

    /// Per port: whether its owner gives power into its network by its role (electrical ones).
    pub fn source_roles(&self, owners: &[Owner]) -> Vec<bool> {
        owners.iter().map(|o| matches!(o, Owner::Machine(_, role) if ELECTRIC_SOURCES.contains(role))).collect()
    }

    // ---- failures ----

    /// A part destroyed, as a hit does.
    pub fn destroy(&mut self, part: usize) {
        let p = &mut self.s.parts[part];
        p.alive = false;
        p.working = false;
        self.s.refresh();
    }

    /// A part put back as new.
    pub fn mend(&mut self, part: usize) {
        let p = &mut self.s.parts[part];
        p.alive = true;
        p.working = true;
        p.hp = p.max_hp;
        self.s.refresh();
    }

    /// The balance of every network this tick: what its producers gave, less what went into its
    /// stores (or plus what came out of them), is what its consumers got and what it leaked. The
    /// networks out of balance, with by how much.
    pub fn out_of_balance(&self) -> Vec<String> {
        let mut bad = Vec::new();
        for (n, net) in self.ship.nets.iter().enumerate() {
            let (mut gave, mut stored, mut got, mut size) = (0.0, 0.0, 0.0, 0.0f64);
            for (_, p) in self.ports_on(n) {
                gave += p.gave;
                stored += p.stored;
                got += p.got;
                size += p.gave.abs() + p.stored.abs() + p.got.abs();
            }
            let leaked: f64 = net.islands.iter().map(|i| i.leaked).sum();
            let off = gave - stored - got - leaked;
            if !off.is_finite() || off.abs() > 1e-9 * (size + leaked) + 1e-12 {
                bad.push(format!("{} t {:.2}: red {} ({}): dado {gave:.6} − guardado {stored:.6} − servido {got:.6} − fugado {leaked:.6} = {off:.3e}", self.id(), self.ship.t, net.name, net.medium));
            }
        }
        bad
    }

    /// What must hold of a ship at any tick, whatever is done to it: every signal a number, every
    /// port's figures numbers, every store within its bounds, every joint within its travel, the
    /// air of every compartment a real gas. What does not, by name.
    pub fn broken(&self) -> Vec<String> {
        let mut bad = Vec::new();
        let id = self.id();
        let t = self.ship.t;
        for (i, v) in self.ship.store.values().iter().enumerate() {
            if !v.is_finite() {
                bad.push(format!("{id} t {t:.2}: la señal '{}' vale {v}", self.name(SignalId(i as u32))));
            }
        }
        for (k, p) in self.ship.ports.iter().enumerate() {
            let all = [p.produce, p.demand, p.store_out, p.store_in, p.potential, p.got, p.share, p.gave, p.stored, p.level, p.leaked];
            if all.iter().any(|v| !v.is_finite()) || p.produce < 0.0 || p.demand < 0.0 || p.store_out < 0.0 || p.store_in < 0.0 || !(0.0..=1.0 + 1e-9).contains(&p.share) {
                bad.push(format!("{id} t {t:.2}: el puerto de {} pide o recibe algo imposible: {p:?}", self.port_name(k)));
            }
        }
        // a store says how full it is as a share (never under nothing nor over full) or by weight
        for &(sig, share) in &self.vitals {
            let v = self.get(sig);
            if v < -1e-9 || (share && v > 1.0 + 1e-6) {
                bad.push(format!("{id} t {t:.2}: {} = {v} (fuera de {})", self.name(sig), if share { "0..1" } else { "lo posible" }));
            }
        }
        for (j, plan) in self.kind.joints.iter().enumerate() {
            let jv = &self.ship.joints[j];
            if !jv.q.is_finite() || !jv.qd.is_finite() || jv.q < jv.lo - 1e-6 || jv.q > jv.hi + 1e-6 {
                bad.push(format!("{id} t {t:.2}: la articulación {} está en {} (recorrido {}..{})", plan.id, jv.q, jv.lo, jv.hi));
            }
        }
        for (c, plan) in self.kind.compartments.iter().enumerate() {
            let a = &self.ship.atmos.air[c];
            if [a.o2, a.n2, a.co2].iter().any(|v| !v.is_finite() || *v < 0.0) || !a.t.is_finite() || a.t <= 0.0 {
                bad.push(format!("{id} t {t:.2}: el aire de {} no es un gas: {a:?}", plan.id));
            }
        }
        if !self.s.force.is_finite() || !self.s.torque.is_finite() {
            bad.push(format!("{id} t {t:.2}: empuje {:?}, par {:?}", self.s.force, self.s.torque));
        }
        bad
    }

    /// Everything that is its state, as one number: two ships with the same have done the same.
    pub fn fingerprint(&self) -> u64 {
        let mut h = Fnv::default();
        for v in self.ship.store.values() {
            h.f64(*v);
        }
        for j in &self.ship.joints {
            h.f64(j.q);
            h.f64(j.qd);
        }
        let mut state = Vec::new();
        for m in &self.ship.machines {
            state.clear();
            m.m.save(&mut state);
            state.iter().for_each(|v| h.f64(*v));
        }
        for a in &self.ship.actuators {
            state.clear();
            a.save(&mut state);
            state.iter().for_each(|v| h.f64(*v));
        }
        for c in &self.ship.panels.controls {
            for v in [c.st.x, c.st.y, c.st.v, c.st.t, f64::from(c.st.flags)] {
                h.f64(v);
            }
        }
        for a in &self.ship.atmos.air {
            for v in [a.o2, a.n2, a.co2, a.t] {
                h.f64(v);
            }
        }
        for p in &self.s.parts {
            h.f64(f64::from(p.hp));
            h.f64(f64::from(u8::from(p.alive)));
        }
        for v in self.s.force.to_array().into_iter().chain(self.s.torque.to_array()) {
            h.f64(f64::from(v));
        }
        h.0
    }
}

/// FNV-1a over the bits of what it is fed.
pub struct Fnv(pub u64);

impl Default for Fnv {
    fn default() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
}

impl Fnv {
    pub fn f64(&mut self, v: f64) {
        for b in v.to_bits().to_le_bytes() {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

// ---------------------------------------------------------------- conditions

/// The numbers an expression is written with, in SI ("22 V" is 22, "5 kPa" is 5 000): the values
/// its signals have to cross for it to change its mind.
pub fn constants(src: &str) -> Vec<f64> {
    let b: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        // (a digit inside a name — `bateria_1.soc` — is not a number)
        if !b[i].is_ascii_digit() || (i > 0 && (b[i - 1].is_alphanumeric() || matches!(b[i - 1], '_' | '.' | '#'))) {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < b.len() && (b[j].is_ascii_digit() || b[j] == '.') {
            j += 1;
        }
        let number: String = b[i..j].iter().collect();
        let from = if b.get(j) == Some(&' ') { j + 1 } else { j };
        let mut to = from;
        while to < b.len() && (b[to].is_alphabetic() || matches!(b[to], '°' | '%' | '/' | '·') || (to > from && b[to].is_ascii_digit())) {
            to += 1;
        }
        let unit: String = b[from..to].iter().collect();
        let with_unit = if unit.is_empty() { None } else { lunar_signals::units::parse(&format!("{number} {unit}")).ok().map(|x| x.0) };
        if let Some(v) = with_unit.or_else(|| number.parse().ok()) {
            out.push(v);
        }
        i = j.max(i + 1);
    }
    out
}

/// A copy of a ship's signals to try conditions on: its inputs given values round the numbers
/// the condition is written with, again and again, to see what it can say.
pub struct Trial {
    pub store: lunar_signals::Store,
    eval: lunar_signals::Eval,
    rng: Rng,
    t: f64,
}

impl Trial {
    pub fn new(r: &Rig, seed: u64) -> Trial {
        Trial { store: r.ship.store.clone(), eval: lunar_signals::Eval::default(), rng: Rng(seed), t: 0.0 }
    }

    /// The values worth trying for the inputs of expressions written as `sources`.
    pub fn candidates(sources: &[&str]) -> Vec<f64> {
        let mut c = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 0.5, -1.0, 1e9, -1e9];
        for v in sources.iter().flat_map(|s| constants(s)) {
            let d = (v.abs() * 0.02).max(0.011);
            c.extend([v, v + d, v - d, -v, -v + d, -v - d]);
        }
        c.sort_by(f64::total_cmp);
        c.dedup();
        c
    }

    /// Every input given one of the candidates.
    pub fn shake(&mut self, inputs: &[SignalId], candidates: &[f64]) {
        for &s in inputs {
            let v = candidates[self.rng.below(candidates.len())];
            self.store.set(s, v);
        }
    }

    /// Whether `p` holds now, a long while after its inputs were last moved (so that what waits —
    /// `for`, `lag`, `hold` — has waited); `slots` are its state.
    pub fn holds(&mut self, p: &lunar_signals::Program, slots: &mut [f64]) -> bool {
        self.t += 1000.37;
        self.eval.run(p, &self.store, slots, 1000.0, self.t) >= 0.5
    }

    /// Whether `p` can be seen both to hold and not to, in `tries` assignments of its inputs:
    /// (seen true, seen false).
    pub fn both_ways(&mut self, p: &lunar_signals::Program, tries: usize) -> (bool, bool) {
        let candidates = Trial::candidates(&[&p.source]);
        let mut slots = vec![0.0; p.slots];
        let (mut yes, mut no) = (false, false);
        for _ in 0..tries {
            self.shake(&p.inputs, &candidates);
            // (twice: what answers to an edge says so once, what waits says so the second time)
            for _ in 0..2 {
                if self.holds(p, &mut slots) { yes = true } else { no = true }
            }
            if yes && no {
                break;
            }
        }
        (yes, no)
    }

    /// The signals put so that `p` holds (or does not): whether such values were found.
    pub fn make(&mut self, p: &lunar_signals::Program, want: bool, tries: usize) -> bool {
        let candidates = Trial::candidates(&[&p.source]);
        let mut slots = vec![0.0; p.slots];
        for _ in 0..tries {
            self.shake(&p.inputs, &candidates);
            self.holds(p, &mut slots);
            if self.holds(p, &mut slots) == want {
                return true;
            }
        }
        false
    }
}
