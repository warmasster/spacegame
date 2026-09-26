// Ship definitions are data: hull panels, consoles, controls, doors, power routing. The same
// definition drives the server rules (damage, repair, power, interlocks), the client visuals and
// the physics colliders, so they can never disagree. See hauler.ts for the first ship.

import {
  area2,
  centroid,
  clipPoly,
  cross,
  dot,
  type Frame,
  fromFrame,
  insetPoly,
  len,
  madd,
  norm,
  pointInPoly,
  polyNormal,
  scale,
  sub,
  toFrame,
  type V2,
  type V3,
} from './geom.js';

/** hull = outer skin (wall/roof), glass = window, floor = deck plate, bulkhead = interior wall. */
export type PanelKind = 'hull' | 'glass' | 'floor' | 'bulkhead';


/** A breakable, repairable plate. Flat convex prism: polygon in (u, v) around `c`, ±t/2 along n. */
export interface PanelDef extends Frame {
  index: number;
  /** Human-readable code painted on the panel, e.g. "CG-R2-3". */
  id: string;
  kind: PanelKind;
  zone: string;
  /** Convex, counter-clockwise seen from +n, metres. `n` points outside (walls) or up (floors). */
  poly: V2[];
  t: number;
  maxHp: number;
  /** Power subsystems whose conduit runs behind this panel (cut when it is destroyed). */
  conduits: SubsystemId[];
  /** Bulkheads: the compartment on the other side (a hole joins the two). */
  other?: string;
}

/** Power circuits (breaker + conduit + priority), see power.ts. */
export type SubsystemId = 'lights' | 'ext' | 'doors' | 'hyd' | 'avionics' | 'shield' | 'prop' | 'life' | 'cool' | 'sensors' | 'weapons' | 'grav';

export interface SubsystemDef {
  id: SubsystemId;
  label: string;
  /** Breaker switch key. */
  breaker: string;
  /** Conduit runs from the breaker cabinet (polylines, ship space). */
  routes: V3[][];
  color: number;
  /** Breaker trip rating (kW): a load above it for 2 s opens the breaker. */
  rating: number;
  /** Load always drawn while the circuit is live (kW). */
  base: number;
  /** Switch key of the circuit's priority selector (0 ALTA, 1 NORMAL, 2 BAJA: shed first). */
  priority: string;
}

export type ControlKind = 'button' | 'toggle' | 'lever' | 'breaker' | 'master' | 'rotary' | 'cover' | 'valve' | 'bezel';

/**
 * How a click changes the control's state key: toggle 0↔1, reset → 0, cycle → next position
 * (the mouse wheel steps it up/down), pulse → 1 for the simulation to consume, set → `value`.
 */
export type ControlAction = 'toggle' | 'reset' | 'cycle' | 'pulse' | 'set';

/** A clickable control. Several controls may drive the same state `key` (e.g. ramp buttons). */
export interface ControlDef extends Frame {
  index: number;
  id: string;
  key: string;
  action: ControlAction;
  /** Value written by `set` controls (MFD page buttons). */
  value?: number;
  kind: ControlKind;
  /** Printed on the console. */
  label: string;
  /** Shown in the helmet HUD when aimed at. */
  name: string;
  /** State texts per position (index = value of `key`). */
  states: string[];
  /** Subsystem that must be powered for the control to do anything. */
  requires?: SubsystemId;
  /** Key of the flip cover over it: while the cover is shut the control can't be reached. */
  guard?: string;
  console: string;
  /** Panel this control is mounted on (-1 = free-standing): destroyed panel → control lost. */
  host: number;
  /** Hit box half extents along u, v, n. */
  half: V3;
}

// -----------------------------------------------------------------------------------------------
// Systems: parts, propellant network, compartments (see sim.ts / systems/*.ts)
// -----------------------------------------------------------------------------------------------

export type PartType =
  | 'reactor'
  | 'battery'
  | 'apu'
  | 'coolpump'
  | 'radiator'
  | 'tank'
  | 'engine'
  | 'rcs'
  | 'o2gen'
  | 'scrubber'
  | 'gas'
  | 'radar'
  | 'antenna'
  | 'turret'
  | 'grav'
  | 'loader';

/**
 * A breakable, repairable machine: an oriented box in ship space (blasts measure distance to it,
 * the crosshair and the welder aim at it), its compartment (null = outside), its power circuit,
 * and type parameters read by the systems code.
 */
export interface PartDef {
  index: number;
  id: string;
  type: PartType;
  /** HUD name. */
  name: string;
  c: V3;
  half: V3;
  /** Rotation about +Y (ship space). */
  yaw: number;
  zone: string | null;
  maxHp: number;
  circuit?: SubsystemId;
  /** Moving mount (nacelle) the part rides on. */
  mount?: string;
  /** Damage taken from blasts is multiplied by this (armour < 1). */
  soft: number;
  /** Propellant consumers: network node they draw from. */
  feed?: string;
  p: Record<string, number>;
}

/** Propellant network: tanks and consumers are parts, manifolds are plain nodes. */
export interface FluidNetDef {
  manifolds: string[];
  /** Pipe between two nodes (part ids or manifolds), open while `valve` is 1 (no valve = always). */
  pipes: Array<{ a: string; b: string; valve?: string }>;
}

/** A pressurised compartment (one per zone). */
export interface CompartmentDef {
  id: string;
  label: string;
  /** Free gas volume (m³). */
  volume: number;
}

/**
 * Gas path between two compartments or to vacuum (b = null): doors and the ramp open with their
 * mover, vents with their valve, ducts (air circulation) with their damper while the fans run.
 */
export interface OpeningDef {
  id: string;
  kind: 'door' | 'ramp' | 'vent' | 'duct';
  a: string;
  b: string | null;
  /** Mover or valve key that opens it. */
  key: string;
  /** Fully open area (m²). */
  area: number;
}

/** A moving part driven by a switch: travels toward it while its circuit is powered. */
export interface MoverDef {
  key: string;
  /** Fraction per second. */
  rate: number;
  circuit: SubsystemId;
  /** Extra power while travelling (kW). */
  load: number;
}

export interface ConsoleDef extends Frame {
  id: string;
  w: number;
  h: number;
  title: string;
  host: number;
  /** Box depth behind the face (m). */
  depth: number;
}

export type ScreenPage = 'status' | 'hull' | 'power' | 'fuel' | 'atmos' | 'engines' | 'reactor' | 'alerts' | 'flight' | 'nav' | 'radar' | 'weapons';

/** Multi-function display: `pages` selectable with its bezel buttons (switch key `mfd.<id>`). */
export interface ScreenDef extends Frame {
  id: string;
  w: number;
  h: number;
  pages: ScreenPage[];
  host: number;
}

/** Double sliding door in a bulkhead. `c` = bottom centre of the opening, `n` = opening normal. */
export interface DoorDef {
  key: string;
  c: V3;
  n: V3;
  w: number;
  h: number;
  /** Leaves sit this far along n from the bulkhead plane. */
  offset: number;
}

/** Rear ramp hinged on the floor edge; closed = vertical, open = resting on the ground. */
export interface RampDef {
  key: string;
  hinge: V3;
  w: number;
  length: number;
  t: number;
}

/** Roller shutter over a window: deployed it covers w × h around `c`, retracted it rolls up to the top edge (+v). */
export interface ShieldPlate extends Frame {
  w: number;
  h: number;
}

export interface ZoneDef {
  id: string;
  label: string;
  min: V3;
  max: V3;
  /** Ceiling light position. */
  lights: V3[];
  lightKey: string;
}

/** A seat: where the astronaut's feet/root go when sitting, facing (ship yaw), and where it stands up. */
export interface SeatDef {
  id: string;
  name: string;
  root: V3;
  yaw: number;
  exit: V3;
}

/** A box in seat space (x right, y up, z toward the backrest), for colliders and picking. */
export interface SeatBox {
  c: V3;
  half: V3;
}

/**
 * Crew seat for a suited astronaut, measured on the seated rig (hips 0.6 m, PLSS back 0.48 m behind
 * the root, helmet back 0.25 m): pan under the thighs, a PLSS well between two side wings, the
 * dock plate behind the pack and a headrest on the top bar that meets the helmet above the pack.
 * Shared by the ship view, its colliders and the crosshair pick so they can never disagree.
 */
export const SEAT_BOXES: Record<'pan' | 'dock' | 'wingL' | 'wingR' | 'head', SeatBox> = {
  pan: { c: [0, 0.43, 0.02], half: [0.3, 0.05, 0.47] },
  dock: { c: [0, 0.98, 0.535], half: [0.335, 0.5, 0.035] },
  wingL: { c: [-0.3, 0.95, 0.36], half: [0.035, 0.47, 0.17] },
  wingR: { c: [0.3, 0.95, 0.36], half: [0.035, 0.47, 0.17] },
  head: { c: [0, 1.46, 0.385], half: [0.16, 0.1, 0.13] },
};

/** Whole-seat box for the crosshair (seat space). */
export const SEAT_PICK: SeatBox = { c: [0, 0.78, 0.06], half: [0.34, 0.78, 0.51] };

/** Seat-space box → ship-space frame (u = seat right, v = up, n = toward the backrest). */
export function seatFrame(seat: SeatDef, b: SeatBox): Frame & { half: V3 } {
  const cs = Math.cos(seat.yaw);
  const sn = Math.sin(seat.yaw);
  const u: V3 = [cs, 0, -sn];
  const n: V3 = [sn, 0, cs];
  const c: V3 = [seat.root[0] + u[0] * b.c[0] + n[0] * b.c[2], seat.root[1] + b.c[1], seat.root[2] + u[2] * b.c[0] + n[2] * b.c[2]];
  return { c, u, v: [0, 1, 0], n, half: b.half };
}

/** Loose cargo: a dynamic box (centre, half extents, yaw, mass). */
export interface CargoDef {
  pos: V3;
  half: V3;
  yaw: number;
  mass: number;
  paint: 'orange' | 'grey';
}

export interface ExtLightDef {
  kind: 'nav-red' | 'nav-green' | 'strobe' | 'beacon' | 'landing';
  /** Point on the hull surface the fixture sits on (ship space). */
  pos: V3;
  /** Outward surface normal there: the fixture's base lies on the surface, the lens stands along it. */
  n: V3;
  /** Beam direction (landing lights). */
  dir?: V3;
}

export interface ShipDef {
  id: string;
  name: string;
  registry: string;
  /** Floor height above the ground on its gear (m). */
  floorHeight: number;
  panels: PanelDef[];
  controls: ControlDef[];
  consoles: ConsoleDef[];
  screens: ScreenDef[];
  doors: DoorDef[];
  ramp: RampDef;
  shield: { key: string; plates: ShieldPlate[] };
  gear: { key: string; legs: V3[] };
  zones: ZoneDef[];
  seats: SeatDef[];
  cargo: CargoDef[];
  extLights: ExtLightDef[];
  subsystems: SubsystemDef[];
  parts: PartDef[];
  fluid: FluidNetDef;
  compartments: CompartmentDef[];
  openings: OpeningDef[];
  movers: MoverDef[];
  /** Annunciator lamp grid on a console (lamp names = Alert.lamp groups). */
  annunciator: { console: string; at: V2; cols: number; cell: V2; lamps: string[] };
  defaults: Record<string, number>;
  /** Hull cross-sections (for the decor/structure builder). */
  modules: ModuleDef[];
  /** Ship-space bounds of everything (for culling / "near the ship" tests). */
  bounds: { min: V3; max: V3 };
}

/** A hull section extruded along z with a constant cross-section. */
export interface ModuleDef {
  zone: string;
  z0: number;
  z1: number;
  profile: V2[];
  cols: number;
}

// -----------------------------------------------------------------------------------------------
// Builder
// -----------------------------------------------------------------------------------------------

export const PANEL_GAP = 0.018;

export interface PanelOpts {
  id: string;
  kind: PanelKind;
  zone: string;
  /** Bulkheads: compartment on the other side. */
  other?: string;
  t: number;
  hp?: number;
  /** Where the given polygon lies: 'inner' = interior face (skin grows outward), 'top' = top
   *  face (plate hangs below), 'mid' = centred. */
  face: 'inner' | 'top' | 'mid';
}

const HP: Record<PanelKind, number> = { hull: 100, glass: 60, floor: 120, bulkhead: 80 };

/** Top outline of a profile (x ascending) evaluated at x. */
export function profileTop(profile: V2[], x: number) {
  for (let i = 0; i < profile.length - 1; i++) {
    const a = profile[i];
    const b = profile[i + 1];
    if (b[0] <= a[0]) continue; // vertical side
    if (x >= a[0] - 1e-6 && x <= b[0] + 1e-6) return a[1] + ((x - a[0]) / (b[0] - a[0])) * (b[1] - a[1]);
  }
  return 0;
}

export class ShipBuilder {
  readonly panels: PanelDef[] = [];

  /** Add a panel from a planar convex polygon (ship space). */
  addPoly(pts: V3[], outward: V3, o: PanelOpts) {
    if (pts.length < 3) return;
    let n = polyNormal(pts);
    if (dot(n, outward) < 0) n = scale(n, -1);
    // v = world up projected on the plane (walls), else toward the nose (floors / roofs)
    let v = sub([0, 1, 0], scale(n, n[1]));
    if (len(v) < 0.3) v = sub([0, 0, -1], scale(n, -n[2]));
    v = norm(v);
    const u = cross(v, n);
    const c0 = centroid(pts);
    let poly: V2[] = pts.map((p) => [dot(sub(p, c0), u), dot(sub(p, c0), v)]);
    if (area2(poly) < 0) poly.reverse();
    if (Math.abs(area2(poly)) < 0.02) return;
    poly = insetPoly(poly, PANEL_GAP / 2);
    const off = o.face === 'inner' ? o.t / 2 : o.face === 'top' ? -o.t / 2 : 0;
    const c = madd(c0, n, off);
    this.panels.push({ index: this.panels.length, id: o.id, kind: o.kind, zone: o.zone, other: o.other, c, u, v, n, poly, t: o.t, maxHp: o.hp ?? HP[o.kind], conduits: [] });
  }

  /**
   * Walls/roof of a module: every profile segment split into rows, the length into `cols`.
   * `labels[i]` names profile segment i; `rows[i]` its row count. Optional clip plane keeps
   * dot(p - origin, n) <= 0 (nose cut).
   */
  strip(
    m: ModuleDef,
    prefix: string,
    labels: string[],
    rows: number[],
    kindAt: (seg: string, row: number, col: number) => PanelKind,
    t: number,
    clip?: { origin: V3; n: V3 },
  ) {
    const P = m.profile;
    const cy = Math.max(...P.map((p) => p[1])) / 2;
    for (let s = 0; s < P.length - 1; s++) {
      const a = P[s];
      const b = P[s + 1];
      const nr = rows[s];
      const mid: V2 = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
      const outward: V3 = [mid[0], mid[1] - cy, 0];
      for (let r = 0; r < nr; r++) {
        const pa: V2 = [a[0] + ((b[0] - a[0]) * r) / nr, a[1] + ((b[1] - a[1]) * r) / nr];
        const pb: V2 = [a[0] + ((b[0] - a[0]) * (r + 1)) / nr, a[1] + ((b[1] - a[1]) * (r + 1)) / nr];
        for (let c = 0; c < m.cols; c++) {
          const za = m.z0 + ((m.z1 - m.z0) * c) / m.cols;
          const zb = m.z0 + ((m.z1 - m.z0) * (c + 1)) / m.cols;
          let pts: V3[] = [
            [pa[0], pa[1], za],
            [pb[0], pb[1], za],
            [pb[0], pb[1], zb],
            [pa[0], pa[1], zb],
          ];
          if (clip) pts = clipPoly(pts, clip.origin, clip.n);
          const kind = kindAt(labels[s], r, c);
          this.addPoly(pts, outward, { id: `${prefix}-${labels[s]}${nr > 1 ? r + 1 : ''}-${c + 1}`, kind, zone: m.zone, t: kind === 'glass' ? 0.05 : t, face: 'inner' });
        }
      }
    }
  }

  /** Deck plates: nx × nz grid over [x0,x1] × [z0,z1] at y = 0 (top face). */
  floor(zone: string, prefix: string, x0: number, x1: number, z0: number, z1: number, nx: number, nz: number, t: number) {
    for (let j = 0; j < nz; j++) {
      for (let i = 0; i < nx; i++) {
        const xa = x0 + ((x1 - x0) * i) / nx;
        const xb = x0 + ((x1 - x0) * (i + 1)) / nx;
        const za = z0 + ((z1 - z0) * j) / nz;
        const zb = z0 + ((z1 - z0) * (j + 1)) / nz;
        this.addPoly(
          [
            [xa, 0, za],
            [xb, 0, za],
            [xb, 0, zb],
            [xa, 0, zb],
          ],
          [0, 1, 0],
          { id: `${prefix}-F${j + 1}${String.fromCharCode(65 + i)}`, kind: 'floor', zone, t, face: 'top' },
        );
      }
    }
  }

  /**
   * End cap / bulkhead at plane z: the area inside `outer` minus the opening `inner`, cut into
   * convex pieces (vertical strips merged while they share the same lower edge) and rows.
   */
  cap(
    zone: string,
    prefix: string,
    z: number,
    outer: V2[],
    inner: V2[] | null,
    o: { outwardZ: number; kind: PanelKind; t: number; rowH: number; splitX?: number[]; clip?: { origin: V3; n: V3 }; face: 'inner' | 'mid'; other?: string },
  ) {
    const wOut = outer[outer.length - 1][0];
    const wIn = inner ? inner[inner.length - 1][0] : 0;
    const xs = [...new Set([...outer.map((p) => p[0]), ...(inner ?? []).map((p) => p[0]), ...(o.splitX ?? [])])]
      .filter((x) => x >= -wOut - 1e-6 && x <= wOut + 1e-6)
      .sort((a, b) => a - b);
    // lower edge identity per interval: 'z' = floor, or the inner segment index
    const lowerKey = (xm: number) => {
      if (!inner || Math.abs(xm) >= wIn) return 'z';
      for (let i = 0; i < inner.length - 1; i++) if (inner[i + 1][0] > inner[i][0] && xm >= inner[i][0] && xm <= inner[i + 1][0]) return `i${i}`;
      return 'z';
    };
    const lower = (x: number, key: string) => (key === 'z' ? 0 : profileTop(inner!, x));
    const groups: Array<{ key: string; xs: number[] }> = [];
    for (let i = 0; i < xs.length - 1; i++) {
      const xa = xs[i];
      const xb = xs[i + 1];
      if (xb - xa < 1e-4) continue;
      const key = lowerKey((xa + xb) / 2);
      const last = groups[groups.length - 1];
      const forced = o.splitX?.some((s) => Math.abs(s - xa) < 1e-6);
      if (last && last.key === key && !forced) last.xs.push(xb);
      else groups.push({ key, xs: [xa, xb] });
    }
    let n = 0;
    const H = Math.max(...outer.map((p) => p[1]));
    for (const g of groups) {
      const xa = g.xs[0];
      const xb = g.xs[g.xs.length - 1];
      const poly: V3[] = [
        [xa, lower(xa, g.key), z],
        [xb, lower(xb, g.key), z],
        ...g.xs
          .slice()
          .reverse()
          .map((x): V3 => [x, profileTop(outer, x), z]),
      ];
      if (Math.abs(poly[0][1] - poly[poly.length - 1][1]) < 1e-4 && Math.abs(poly[1][1] - poly[2][1]) < 1e-4) continue;
      for (let y0 = 0; y0 < H - 1e-4; y0 += o.rowH) {
        let pts = clipPoly(poly, [0, y0, 0], [0, -1, 0]);
        pts = clipPoly(pts, [0, y0 + o.rowH, 0], [0, 1, 0]);
        if (o.clip) pts = clipPoly(pts, o.clip.origin, o.clip.n);
        if (pts.length < 3) continue;
        const before = this.panels.length;
        this.addPoly(pts, [0, 0, o.outwardZ], { id: `${prefix}-${++n}`, kind: o.kind, zone, other: o.other, t: o.t, face: o.face });
        if (this.panels.length === before) n--;
      }
    }
  }
}

/** Panel whose face a point sits on (within `maxDist` along the panel normal). */
export function hostPanel(panels: PanelDef[], p: V3, maxDist = 0.3) {
  let best = -1;
  let bestD = maxDist;
  for (const pn of panels) {
    const l = toFrame(pn, p);
    const d = Math.abs(l[2]);
    if (d < bestD && pointInPoly(pn.poly, l[0], l[1])) {
      bestD = d;
      best = pn.index;
    }
  }
  return best;
}

/** Mark the panels every conduit run passes behind. */
export function routeConduits(panels: PanelDef[], subsystems: SubsystemDef[]) {
  for (const s of subsystems) {
    for (const route of s.routes) {
      for (let i = 0; i < route.length - 1; i++) {
        const a = route[i];
        const b = route[i + 1];
        const steps = Math.max(1, Math.ceil(len(sub(b, a)) / 0.15));
        for (let k = 0; k <= steps; k++) {
          const p = madd(a, sub(b, a), k / steps);
          for (const pn of panels) {
            const l = toFrame(pn, p);
            if (Math.abs(l[2]) < pn.t / 2 + 0.4 && pointInPoly(pn.poly, l[0], l[1]) && !pn.conduits.includes(s.id)) pn.conduits.push(s.id);
          }
        }
      }
    }
  }
}

/** Console-local placement → ship-space frame. */
export function onConsole(con: Frame, x: number, y: number, lift = 0): Frame {
  return { c: fromFrame(con, x, y, lift), u: con.u, v: con.v, n: con.n };
}

/** Frame of a flat board facing `n` with its "up" as close to `up` as possible. */
export function boardFrame(c: V3, n: V3, up: V3 = [0, 1, 0]): Frame {
  const nn = norm(n);
  let v = sub(up, scale(nn, dot(up, nn)));
  v = norm(v);
  return { c, u: cross(v, nn), v, n: nn };
}
