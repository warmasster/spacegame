//! Networks of any medium, solved the same way. A network is nodes joined by edges: conduit
//! segments (a cable, a pipe, a duct: alive or cut, maybe leaking), switches (breakers, valves,
//! contactors: open or closed) and taps where machines plug in. Each tick:
//!
//! 1. the edges that are alive and closed make islands (union-find, no allocation once grown);
//! 2. in each island every port has said what it can give, what it wants (by priority), what its
//!    store can take or give and what potential it holds (volts, pascals, kelvin);
//! 3. leaks are served first, then each priority from the top; producers give what is used and
//!    stores take the surplus or cover the deficit; the island's potential follows its regulated
//!    sources or its stores;
//! 4. the current through each measured switch (breakers) is the net load of the side away from
//!    the sources.
//!
//! The units are the medium's (W for power, m³/s for hydraulics, kg/s for propellant and gas,
//! m³/s for air, W for heat); a machine's model turns them into what it needs.
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Medium {
    /// Power (W), potential in volts.
    Electrico,
    /// Hydraulic fluid (m³/s), potential in Pa.
    Hidraulico,
    /// Compressed gas (kg/s), potential in Pa.
    Neumatico,
    /// Propellant (kg/s), potential in Pa.
    Propelente,
    /// Breathing gas from bottles (kg/s), potential in Pa.
    Gas,
    /// Ventilation air (m³/s), potential in Pa (fan head).
    Aire,
    /// Coolant flow (kg/s), potential in Pa.
    Refrigerante,
    /// Heat (W), potential in kelvin.
    Termico,
    /// Data: connected or not.
    Datos,
}

impl Medium {
    pub fn parse(s: &str) -> Option<Medium> {
        Some(match s {
            "electrico" | "eléctrico" | "energia" | "energía" => Medium::Electrico,
            "hidraulico" | "hidráulico" => Medium::Hidraulico,
            "neumatico" | "neumático" => Medium::Neumatico,
            "propelente" | "combustible" | "oxidante" => Medium::Propelente,
            "gas" => Medium::Gas,
            "aire" | "ventilacion" | "ventilación" => Medium::Aire,
            "refrigerante" => Medium::Refrigerante,
            "termico" | "térmico" | "calor" => Medium::Termico,
            "datos" => Medium::Datos,
            _ => return None,
        })
    }

    /// Fluids leak out of a cut conduit; power and data just stop.
    pub fn leaks(self) -> bool {
        matches!(self, Medium::Hidraulico | Medium::Neumatico | Medium::Propelente | Medium::Gas | Medium::Refrigerante)
    }

    /// Unit of its flows and of its potential (for telemetry).
    pub fn units(self) -> (&'static str, &'static str) {
        match self {
            Medium::Electrico => ("W", "V"),
            Medium::Hidraulico => ("L/min", "MPa"),
            Medium::Neumatico | Medium::Propelente | Medium::Gas | Medium::Refrigerante => ("kg/s", "kPa"),
            Medium::Aire => ("L/s", "Pa"),
            Medium::Termico => ("kW", "°C"),
            Medium::Datos => ("", ""),
        }
    }
}

impl fmt::Display for Medium {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// What a port asks and offers this tick (filled by its machine before the solve), and what it
/// got (filled by the solve).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PortIo {
    /// Network and node it plugs into (set by the owner when built).
    pub net: u16,
    pub node: u32,
    // ---- asked ----
    /// Most it can produce (rate).
    pub produce: f64,
    /// What it wants to consume (rate), and how urgently (higher first).
    pub demand: f64,
    pub priority: u8,
    /// Most its store can give / take (rate).
    pub store_out: f64,
    pub store_in: f64,
    /// Potential it holds: a regulated source's set point, or a store's present potential.
    pub potential: f64,
    /// A regulated source holds its potential while it has capacity to spare.
    pub regulated: bool,
    // ---- got ----
    /// Consumption granted (rate).
    pub got: f64,
    /// Share of its demand it got (1 when it asked nothing).
    pub share: f64,
    /// Production used (rate).
    pub gave: f64,
    /// Into (+) or out of (−) its store (rate).
    pub stored: f64,
    /// The island's potential.
    pub level: f64,
    /// Its island has something that produces or stores.
    pub fed: bool,
    /// What leaks out of its island, as this producer's share (rate): a pump's reservoir drains.
    pub leaked: f64,
}

impl PortIo {
    pub fn on(net: u16, node: u32) -> PortIo {
        PortIo { net, node, share: 1.0, ..Default::default() }
    }

    /// Clear what the machine asks (done by the owner before each tick's plan).
    pub fn clear_ask(&mut self) {
        self.produce = 0.0;
        self.demand = 0.0;
        self.priority = 0;
        self.store_out = 0.0;
        self.store_in = 0.0;
        self.potential = 0.0;
        self.regulated = false;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeKind {
    /// A conduit segment (cable, pipe, duct): the owner sets `alive` and `leak`.
    Conduit,
    /// A switch (breaker, valve, contactor, damper): the owner sets `closed`.
    Switch,
    /// A fixed connection (a tap, a busbar).
    Fixed,
}

#[derive(Clone, Copy, Debug)]
pub struct Edge {
    pub a: u32,
    pub b: u32,
    pub kind: EdgeKind,
    pub alive: bool,
    pub closed: bool,
    /// 0 sound .. 1 wide open: share of the island's potential pushing fluid out through it.
    pub leak: f64,
    /// Measure the flow through it (breakers): `flow` after the solve.
    pub measure: bool,
    pub flow: f64,
}

impl Edge {
    pub fn new(a: u32, b: u32, kind: EdgeKind) -> Edge {
        Edge { a, b, kind, alive: true, closed: true, leak: 0.0, measure: false, flow: 0.0 }
    }

    pub fn conducts(&self) -> bool {
        self.alive && (self.kind != EdgeKind::Switch || self.closed)
    }
}

/// One network: its medium, nodes, edges and the per-tick results of its islands.
#[derive(Clone, Debug)]
pub struct Net {
    pub name: String,
    pub medium: Medium,
    /// Nominal potential (V, Pa...), for gauges and sag.
    pub nominal: f64,
    pub nodes: u32,
    pub edges: Vec<Edge>,
    /// Flow lost through leaks per unit of leak and of potential ratio (rate at full potential).
    pub leak_rate: f64,
    // ---- results ----
    /// Island of each node (`u32::MAX`: none).
    pub island: Vec<u32>,
    pub islands: Vec<Island>,
    parent: Vec<u32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Island {
    pub potential: f64,
    pub produced: f64,
    pub demanded: f64,
    pub served: f64,
    /// Lost through leaks this tick (rate).
    pub leaked: f64,
    /// Into (+) / out of (−) its stores.
    pub stored: f64,
    pub fed: bool,
}

impl Net {
    pub fn new(name: &str, medium: Medium, nominal: f64) -> Net {
        Net { name: name.to_string(), medium, nominal, nodes: 0, edges: Vec::new(), leak_rate: 0.0, island: Vec::new(), islands: Vec::new(), parent: Vec::new() }
    }

    pub fn node(&mut self) -> u32 {
        self.nodes += 1;
        self.nodes - 1
    }

    pub fn edge(&mut self, a: u32, b: u32, kind: EdgeKind) -> usize {
        self.edges.push(Edge::new(a, b, kind));
        self.edges.len() - 1
    }

    fn find(parent: &mut [u32], mut a: u32) -> u32 {
        while parent[a as usize] != a {
            let up = parent[parent[a as usize] as usize];
            parent[a as usize] = up;
            a = up;
        }
        a
    }

    fn union(parent: &mut [u32], a: u32, b: u32) {
        let (ra, rb) = (Net::find(parent, a), Net::find(parent, b));
        if ra != rb {
            parent[ra.max(rb) as usize] = ra.min(rb);
        }
    }

    /// Islands of the edges that conduct (`skip_measured`: as if every measured switch were open).
    fn label(&mut self, skip_measured: bool) -> usize {
        let n = self.nodes as usize;
        self.parent.clear();
        self.parent.extend(0..n as u32);
        for e in &self.edges {
            if e.conducts() && !(skip_measured && e.measure) {
                Net::union(&mut self.parent, e.a, e.b);
            }
        }
        self.island.clear();
        self.island.resize(n, u32::MAX);
        let mut count = 0u32;
        for i in 0..n {
            let r = Net::find(&mut self.parent, i as u32) as usize;
            if self.island[r] == u32::MAX {
                self.island[r] = count;
                count += 1;
            }
            self.island[i] = self.island[r];
        }
        count as usize
    }
}

//// Per-island sums of one solve.
#[derive(Clone, Copy, Debug, Default)]
struct Acc {
    prod: f64,
    s_out: f64,
    s_in: f64,
    demand: f64,
    reg_pot: f64,
    store_pot: f64,
    hint: f64,
    leak: f64,
    gave: f64,
    stored: f64,
    potential: f64,
    leaked: f64,
    fed: bool,
}

/// Reused between ticks: solves every network with no allocation once grown.
#[derive(Default)]
pub struct Solver {
    acc: Vec<Acc>,
    /// Distinct priorities of the net (highest first) and demand and share per (island, priority).
    prios: Vec<u8>,
    want: Vec<f64>,
    share: Vec<f64>,
    zone_load: Vec<f64>,
    zone_prod: Vec<f64>,
    zone_edges: Vec<(u32, u32, usize)>,
    visit: Vec<bool>,
    parent_edge: Vec<usize>,
    order: Vec<u32>,
    roots: Vec<u32>,
}

impl Solver {
    /// Solve `net` (number `id`) for `ports`: those with `net == id` take part. Cost: one pass over
    /// the ports and the edges, plus islands x priorities.
    pub fn solve(&mut self, id: u16, net: &mut Net, ports: &mut [PortIo]) {
        let count = net.label(false);
        self.acc.clear();
        self.acc.resize(count, Acc::default());
        // leaks: what cut conduits lose, an extra top-priority demand of their island
        if net.medium.leaks() {
            for e in &net.edges {
                if e.kind == EdgeKind::Conduit && e.leak > 0.0 {
                    let i = net.island[e.a as usize];
                    if i != u32::MAX {
                        self.acc[i as usize].leak += e.leak;
                    }
                }
            }
        }
        // pass 1: sums per island, and the priorities in play
        self.prios.clear();
        for p in ports.iter().filter(|p| p.net == id) {
            let a = &mut self.acc[net.island[p.node as usize] as usize];
            a.prod += p.produce;
            a.s_out += p.store_out;
            a.s_in += p.store_in;
            a.demand += p.demand;
            if p.regulated && p.produce > 0.0 {
                a.reg_pot = a.reg_pot.max(p.potential);
            } else if p.store_out > 0.0 || p.store_in > 0.0 {
                a.store_pot = a.store_pot.max(p.potential);
            } else if p.produce > 0.0 {
                a.hint = a.hint.max(p.potential);
            }
            if p.demand > 0.0 && !self.prios.contains(&p.priority) {
                self.prios.push(p.priority);
            }
        }
        self.prios.sort_unstable_by(|a, b| b.cmp(a));
        let np = self.prios.len().max(1);
        self.want.clear();
        self.want.resize(count * np, 0.0);
        self.share.clear();
        self.share.resize(count * np, 1.0);
        for p in ports.iter().filter(|p| p.net == id && p.demand > 0.0) {
            let k = self.prios.iter().position(|&q| q == p.priority).unwrap_or(0);
            self.want[net.island[p.node as usize] as usize * np + k] += p.demand;
        }
        // pass 2: per island, leaks then each priority from the top; producers and stores
        net.islands.clear();
        for (i, a) in self.acc.iter_mut().enumerate() {
            a.fed = a.prod > 0.0 || a.s_out > 0.0 || a.store_pot > 0.0;
            let pot0 = if a.reg_pot > 0.0 { a.reg_pot } else if a.store_pot > 0.0 { a.store_pot } else { a.hint };
            let ratio = if net.nominal > 0.0 { (pot0 / net.nominal).clamp(0.0, 2.0) } else { 1.0 };
            let leaking = a.leak * net.leak_rate * ratio;
            let mut avail = a.prod + a.s_out;
            let leaked = leaking.min(avail);
            avail -= leaked;
            let mut served = 0.0;
            for k in 0..np {
                let want = self.want[i * np + k];
                let sh = if want > 0.0 { (avail / want).clamp(0.0, 1.0) } else { 1.0 };
                self.share[i * np + k] = sh;
                avail -= want * sh;
                served += want * sh;
            }
            let used = served + leaked;
            (a.gave, a.stored) = if used <= a.prod {
                let charge = (a.prod - used).min(a.s_in);
                (used + charge, charge)
            } else {
                (a.prod, -(used - a.prod).min(a.s_out))
            };
            // regulated while their capacity covers the load, else the stores, else what
            // unregulated sources manage, sagging with the shortfall
            let short = if a.demand + leaking > 0.0 { (used / (a.demand + leaking)).clamp(0.0, 1.0) } else { 1.0 };
            a.potential = if a.reg_pot > 0.0 && a.prod >= used - 1e-9 {
                a.reg_pot
            } else if a.store_pot > 0.0 {
                a.store_pot
            } else if a.fed {
                a.reg_pot.max(a.hint) * (0.7 + 0.3 * short)
            } else {
                0.0
            };
            a.leaked = leaked;
            net.islands.push(Island { potential: a.potential, produced: a.gave, demanded: a.demand, served, leaked, stored: a.stored, fed: a.fed });
        }
        // pass 3: back to the ports
        for p in ports.iter_mut().filter(|p| p.net == id) {
            let i = net.island[p.node as usize] as usize;
            let a = &self.acc[i];
            if p.demand > 0.0 {
                let k = self.prios.iter().position(|&q| q == p.priority).unwrap_or(0);
                p.share = self.share[i * np + k];
                p.got = p.demand * p.share;
            } else {
                p.share = 1.0;
                p.got = 0.0;
            }
            p.gave = if a.prod > 0.0 { p.produce * a.gave / a.prod } else { 0.0 };
            p.stored = if a.stored >= 0.0 {
                if a.s_in > 0.0 { p.store_in * a.stored / a.s_in } else { 0.0 }
            } else if a.s_out > 0.0 {
                p.store_out * a.stored / a.s_out
            } else {
                0.0
            };
            p.fed = a.fed;
            p.level = a.potential;
            p.leaked = if a.prod > 0.0 { a.leaked * p.produce / a.prod } else if a.s_out > 0.0 { a.leaked * p.store_out / a.s_out } else { 0.0 };
        }
        if net.edges.iter().any(|e| e.measure) {
            self.measure(id, net, ports);
        }
    }

    /// Flow through every measured switch: the net load of its side away from the sources.
    fn measure(&mut self, id: u16, net: &mut Net, ports: &[PortIo]) {
        // zones: islands with every measured switch open
        let zones = net.label(true);
        self.zone_load.clear();
        self.zone_load.resize(zones, 0.0);
        self.zone_prod.clear();
        self.zone_prod.resize(zones, 0.0);
        for p in ports.iter().filter(|p| p.net == id) {
            let z = net.island[p.node as usize] as usize;
            self.zone_load[z] += p.got + p.stored.max(0.0);
            self.zone_prod[z] += p.gave + (-p.stored).max(0.0);
        }
        self.zone_edges.clear();
        for (k, e) in net.edges.iter_mut().enumerate() {
            e.flow = 0.0;
            if e.measure && e.conducts() {
                let (a, b) = (net.island[e.a as usize], net.island[e.b as usize]);
                if a != b {
                    self.zone_edges.push((a, b, k));
                }
            }
        }
        // a tree of zones grown from the producing ones: each switch carries its child subtree
        self.visit.clear();
        self.visit.resize(zones, false);
        self.parent_edge.clear();
        self.parent_edge.resize(zones, usize::MAX);
        self.order.clear();
        self.roots.clear();
        self.roots.extend((0..zones as u32).filter(|&z| self.zone_prod[z as usize] > 0.0));
        let prod = &self.zone_prod;
        self.roots.sort_by(|a, b| prod[*b as usize].total_cmp(&prod[*a as usize]));
        for r in self.roots.iter().copied().chain(0..zones as u32) {
            if self.visit[r as usize] {
                continue;
            }
            self.visit[r as usize] = true;
            let mut k = self.order.len();
            self.order.push(r);
            while k < self.order.len() {
                let z = self.order[k];
                for &(a, b, e) in &self.zone_edges {
                    let other = if a == z {
                        b
                    } else if b == z {
                        a
                    } else {
                        continue;
                    };
                    if !self.visit[other as usize] {
                        self.visit[other as usize] = true;
                        self.parent_edge[other as usize] = e;
                        self.order.push(other);
                    }
                }
                k += 1;
            }
        }
        // children first: each zone's net load goes up through its switch
        for &z in self.order.iter().rev() {
            let e = self.parent_edge[z as usize];
            if e == usize::MAX {
                continue;
            }
            let load = (self.zone_load[z as usize] - self.zone_prod[z as usize]).max(0.0);
            net.edges[e].flow = load;
            let (a, b) = (net.island[net.edges[e].a as usize], net.island[net.edges[e].b as usize]);
            let parent = if a == z { b } else { a };
            self.zone_load[parent as usize] += load;
        }
        // back to the real islands for the callers
        net.label(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// battery — bus — breaker — circuit — lamp; generator on the bus.
    #[test]
    fn power_shared_by_priority_and_breaker_current() {
        let mut n = Net::new("bus", Medium::Electrico, 28.0);
        let bus = n.node();
        let circuit = n.node();
        let brk = n.edge(bus, circuit, EdgeKind::Switch);
        n.edges[brk].measure = true;
        let mut ports = vec![PortIo::on(0, bus), PortIo::on(0, circuit), PortIo::on(0, circuit), PortIo::on(0, bus)];
        // battery: can give 1 kW, take 500 W, holds 26 V
        ports[0].store_out = 1000.0;
        ports[0].store_in = 500.0;
        ports[0].potential = 26.0;
        // two loads on the circuit, high and low priority
        ports[1].demand = 600.0;
        ports[1].priority = 2;
        ports[2].demand = 800.0;
        ports[2].priority = 1;
        let mut s = Solver::default();
        s.solve(0, &mut n, &mut ports);
        assert!((ports[1].share - 1.0).abs() < 1e-9);
        assert!((ports[2].got - 400.0).abs() < 1e-9, "the low priority gets the rest");
        assert!((ports[0].stored + 1000.0).abs() < 1e-9);
        assert!((n.edges[brk].flow - 1000.0).abs() < 1e-9, "{}", n.edges[brk].flow);
        assert_eq!(ports[1].level, 26.0);
        // a generator on the bus: everyone fed, the battery charges, the bus regulated
        ports[3].produce = 3000.0;
        ports[3].potential = 28.0;
        ports[3].regulated = true;
        s.solve(0, &mut n, &mut ports);
        assert_eq!(ports[2].share, 1.0);
        assert!((ports[0].stored - 500.0).abs() < 1e-9);
        assert!((ports[3].gave - 1900.0).abs() < 1e-9);
        assert_eq!(ports[1].level, 28.0);
        // open the breaker: the circuit is dark
        n.edges[brk].closed = false;
        s.solve(0, &mut n, &mut ports);
        assert_eq!(ports[1].got, 0.0);
        assert!(!ports[1].fed);
    }

    #[test]
    fn a_cut_pipe_leaks_first() {
        let mut n = Net::new("hid", Medium::Hidraulico, 21e6);
        n.leak_rate = 1e-3;
        let a = n.node();
        let b = n.node();
        let pipe = n.edge(a, b, EdgeKind::Conduit);
        let mut ports = vec![PortIo::on(0, a), PortIo::on(0, b)];
        ports[0].produce = 5e-4;
        ports[0].potential = 21e6;
        ports[0].regulated = true;
        ports[1].demand = 4e-4;
        let mut s = Solver::default();
        s.solve(0, &mut n, &mut ports);
        assert_eq!(ports[1].share, 1.0);
        n.edges[pipe].leak = 0.3;
        s.solve(0, &mut n, &mut ports);
        assert!((n.islands[0].leaked - 3e-4).abs() < 1e-12);
        assert!((ports[1].got - 2e-4).abs() < 1e-12);
        n.edges[pipe].alive = false;
        s.solve(0, &mut n, &mut ports);
        assert_eq!(ports[1].got, 0.0);
    }
}
