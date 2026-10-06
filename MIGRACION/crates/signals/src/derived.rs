//! Derived signals: `name = expression`, evaluated in dependency order. A derived signal whose
//! inputs did not change since its last evaluation (and that does not depend on time) is
//! skipped. Cycles are refused when built.
use crate::{
    expr::{self, Eval, ExprError, Program},
    store::{SignalId, Store, Writer},
};

struct Item {
    target: SignalId,
    prog: Program,
    /// Its state slots in `DerivedSet::state`.
    state: usize,
    /// Sum of its inputs' versions at the last evaluation (u64::MAX: never).
    seen: u64,
}

#[derive(Default)]
pub struct DerivedSet {
    items: Vec<Item>,
    order: Vec<u32>,
    state: Vec<f64>,
    eval: Eval,
    /// Evaluations last tick (diagnostics).
    pub evaluated: u32,
}

impl DerivedSet {
    /// Define every target, then compile (an expression may name any signal of `store`,
    /// including other derived ones) and sort by dependencies.
    pub fn build(store: &mut Store, defs: &[(String, String)]) -> Result<DerivedSet, ExprError> {
        let targets: Vec<SignalId> = defs.iter().map(|(n, _)| store.define(n)).collect();
        let mut items = Vec::with_capacity(defs.len());
        let mut state = 0;
        for (k, (name, src)) in defs.iter().enumerate() {
            let prog = expr::compile_in(src, store).map_err(|e| ExprError(format!("{name}: {}", e.0)))?;
            store.claim(targets[k], Writer::Derived(k as u32)).map_err(ExprError)?;
            items.push(Item { target: targets[k], state, seen: u64::MAX, prog });
            state += items[k].prog.slots;
        }
        // topological order (Kahn) over "reads another derived target"
        let n = items.len();
        let of = |id: SignalId| targets.iter().position(|&t| t == id);
        let mut deps: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut indeg = vec![0usize; n];
        for (k, it) in items.iter().enumerate() {
            for &i in &it.prog.inputs {
                if let Some(d) = of(i) {
                    deps[d].push(k);
                    indeg[k] += 1;
                }
            }
        }
        let mut ready: Vec<usize> = (0..n).filter(|&k| indeg[k] == 0).collect();
        let mut order = Vec::with_capacity(n);
        while let Some(k) = ready.pop() {
            order.push(k as u32);
            for &m in &deps[k] {
                indeg[m] -= 1;
                if indeg[m] == 0 {
                    ready.push(m);
                }
            }
        }
        if order.len() < n {
            let stuck: Vec<&str> = (0..n).filter(|&k| indeg[k] > 0).map(|k| defs[k].0.as_str()).collect();
            return Err(ExprError(format!("derivadas en ciclo: {}", stuck.join(", "))));
        }
        Ok(DerivedSet { items, order, state: vec![0.0; state], eval: Eval::default(), evaluated: 0 })
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Recompute what changed (and everything timed), in order.
    pub fn tick(&mut self, store: &mut Store, dt: f64, t: f64) {
        self.evaluated = 0;
        for &k in &self.order {
            let it = &mut self.items[k as usize];
            let stamp: u64 = it.prog.inputs.iter().map(|&i| u64::from(store.version(i))).sum::<u64>().wrapping_add(it.prog.inputs.len() as u64);
            if !it.prog.timed && it.seen == stamp {
                continue;
            }
            it.seen = stamp;
            let v = self.eval.run(&it.prog, store, &mut self.state[it.state..it.state + it.prog.slots], dt, t);
            store.set(it.target, v);
            self.evaluated += 1;
        }
    }

    /// The state of every stateful function (snapshots).
    pub fn state(&self) -> &[f64] {
        &self.state
    }

    pub fn load_state(&mut self, s: &[f64]) {
        let n = s.len().min(self.state.len());
        self.state[..n].copy_from_slice(&s[..n]);
        for it in &mut self.items {
            it.seen = u64::MAX;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_in_order_and_cycles_refused() {
        let mut s = Store::new();
        s.define_unit("p", "MPa", 2.0e6).unwrap();
        s.define("reconocer");
        let defs = vec![
            ("alarma.general".to_string(), "latch(alarma.baja, reconocer)".to_string()),
            ("alarma.baja".to_string(), "p < 1.5 MPa".to_string()),
        ];
        let mut d = DerivedSet::build(&mut s, &defs).unwrap();
        d.tick(&mut s, 0.05, 0.0);
        let g = s.find("alarma.general").unwrap();
        assert_eq!(s.get(g), 0.0);
        let p = s.find("p").unwrap();
        s.set(p, 1.0e6);
        d.tick(&mut s, 0.05, 0.05);
        assert_eq!(s.get(g), 1.0);
        s.set(p, 2.0e6);
        d.tick(&mut s, 0.05, 0.1);
        assert_eq!(s.get(g), 1.0, "latched until acknowledged");
        let r = s.find("reconocer").unwrap();
        s.set(r, 1.0);
        d.tick(&mut s, 0.05, 0.15);
        assert_eq!(s.get(g), 0.0);
        let mut s2 = Store::new();
        let cyc = vec![("a".to_string(), "b + 1".to_string()), ("b".to_string(), "a + 1".to_string())];
        assert!(DerivedSet::build(&mut s2, &cyc).is_err());
    }
}
