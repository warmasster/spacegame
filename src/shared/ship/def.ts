// Ship definitions are data: hull panels, consoles, controls, doors, power routing, machines and
// furniture. The same definition drives the server rules (damage, repair, power, interlocks), the
// client visuals and the physics colliders, so they can never disagree. See hauler.ts for the
// first ship and docs/SHIPS.md for how to build one.

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
  rotY,
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
  /** Paint scheme on its outer face (0 light, 1 dark, 2 hazard, 3 stripe), see `paintPanels`. */
  paint?: number;
}

/** Power circuit id (breaker + conduit + priority), see modules/power.ts. Each ship names its own. */
export type SubsystemId = string;

export interface SubsystemDef {
  id: SubsystemId;
  label: string;
  /** Breaker switch key. */
  breaker: string;
  /** Conduit runs from the breaker cabinet (polylines, ship space). */
  routes: V3[][];
  color: number;
  /** What hangs off it, one line (breaker help, manual). */
  desc?: string;
  /** Breaker trip rating (kW): a load above it for 2 s opens the breaker. */
  rating: number;
  /** Load always drawn while the circuit is live (kW). */
  base: number;
  /** Switch key of the circuit's priority selector (0 ALTA, 1 NORMAL, 2 BAJA: shed first). */
  priority: string;
}

/**
 * master = lit push-button (master caution), mushroom = red emergency button (SCRAM),
 * cover = flip guard over another control, bezel = MFD page button.
 */
export type ControlKind = 'button' | 'toggle' | 'lever' | 'breaker' | 'master' | 'mushroom' | 'rotary' | 'cover' | 'valve' | 'bezel';

/**
 * How a click changes the control's state key: toggle 0↔1, reset → 0, cycle → next position
 * (right click / wheel step back and forth), pulse → 1 for the simulation to consume,
 * set → `value`.
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
  /** What it does, one line (helmet HUD and manual). */
  help: string;
  /** State texts per position (index = value of `key`). */
  states: string[];
  /** `cycle` knobs: stepping past the last position wraps to the first (selectors) instead of stopping (scales). */
  wrap: boolean;
  /** Subsystem that must be powered for the control to do anything. */
  requires?: SubsystemId;
  /** Key of the flip cover over it: while the cover is shut the control can't be reached. */
  guard?: string;
  console: string;
  /** Panel this control is mounted on (-1 = none): destroyed panel → control lost. */
  host: number;
  /** Machine this control is mounted on (-1 = none): destroyed machine → control lost. */
  hostPart: number;
  /** Hit box half extents along u, v, n. */
  half: V3;
}

// -----------------------------------------------------------------------------------------------
// Systems: parts, propellant network, compartments (see systems.ts and modules/*.ts)
// -----------------------------------------------------------------------------------------------

/**
 * Machine type. Open-ended: a type means something only if a system module claims it
 * (`SystemFactory.parts`, see modules/index.ts); the ship checks refuse unclaimed types.
 */
export type PartType = string;

/** Standard size class of a catalog component (see catalog/): the same machine in four sizes. */
export type Size = 'XS' | 'S' | 'M' | 'L';

/** An oriented box in ship space (yaw about +Y). Parts and props are placed as boxes. */
export interface ShipBox {
  c: V3;
  half: V3;
  yaw: number;
}

/**
 * A breakable, repairable machine: an oriented box in ship space (blasts measure distance to it,
 * the crosshair and the welder aim at it, the client draws its model inside it), its compartment
 * (null = outside), its power circuit, and type parameters read by the systems code.
 */
export interface PartDef extends ShipBox {
  index: number;
  id: string;
  type: PartType;
  /** HUD name. */
  name: string;
  zone: string | null;
  maxHp: number;
  circuit?: SubsystemId;
  /** Moving mount (nacelle) the part rides on. */
  mount?: string;
  /** Damage taken from blasts is multiplied by this (armour < 1). */
  soft: number;
  /** Propellant consumers: network node they draw from. */
  feed?: string;
  /** Model builder (client/ship/models.ts); defaults to the type. */
  model: string;
  /** Collider: its box, a cylinder along z inscribed in it, or none (drawn inside something solid). */
  shape: 'box' | 'cylZ' | 'none';
  /** Numeric parameters read by the part's system module (capacities, power, rates…). */
  p: Record<string, number>;
  /**
   * Switch keys the part's module reads, by role (e.g. `{ run: 'scrub' }`, `{ valve: 'v.gasO2' }`).
   * Roles left out follow the module's naming convention (see `partKey`).
   */
  sw?: Record<string, string>;
  /** Prefix of the part's state variables (default: its id), e.g. 'rx' → 'rx.state', 'rx.temp'. */
  tag?: string;
  /** Part this one serves (a coolant pump / radiator → its reactor). Default: the first one found. */
  link?: string;
  /** Annunciator lamp for the part's own alerts (default: its name in capitals). */
  lamp?: string;
  /** Catalog component it was built from (shared/ship/catalog), if any. */
  component?: string;
  /** Size class of that component. */
  size?: Size;
  /** Manufacturer (catalog/makers.ts): its trim colour on the model. */
  maker?: string;
  /** Dry mass (kg), summed by the flight model (shared/ship/flight.ts). */
  mass: number;
  /** Model generator parameters (variant, detail, count…), see client/ship/models. */
  look: Record<string, number>;
}

/** Switch key a part's module reads for `role`: the part's own `sw[role]`, or the convention. */
export function partKey(part: Pick<PartDef, 'sw'>, role: string, convention: string): string {
  return part.sw?.[role] ?? convention;
}

/** State-variable prefix of a part. */
export function partTag(part: Pick<PartDef, 'id' | 'tag'>): string {
  return part.tag ?? part.id;
}

/**
 * Electrical load switched by a control: while `sw[key]` > 0 the circuit draws `kw` (or
 * `kw[position]` for multi-position selectors). `part`: the machine that draws it (a badly damaged
 * machine that is switched on shorts and draws erratically).
 */
export interface LoadDef {
  key: string;
  circuit: SubsystemId;
  kw: number | number[];
  part?: string;
}

/**
 * Static furniture and structure (dash, pedestal, bunks, galley, chin fairing, fin…): a model from
 * the client model library filling an oriented box. Collider: the box, the convex hull of the
 * model's own outline (`hull`: wedges, fins), or none (thin rails, handholds).
 */
export interface PropDef extends ShipBox {
  index: number;
  id: string;
  model: string;
  zone: string | null;
  collide: 'box' | 'hull' | 'none';
  p: Record<string, number>;
  /** Model generator parameters. */
  look: Record<string, number>;
  /** Mass (kg) for the flight model. */
  mass: number;
}

/** Propellant network: tanks and consumers are parts, manifolds are plain nodes. */
export interface FluidNetDef {
  manifolds: string[];
  /** Pipe between two nodes (part ids or manifolds), open while `valve` is 1 (no valve = always). */
  pipes: Array<{ a: string; b: string; valve?: string }>;
  /** Circuit that runs the boost pumps and the transfer pump (they stop without it). */
  circuit?: SubsystemId;
  /** Transfer pump: selector `key`, position → [from tank, to tank] (null = stopped). */
  transfer?: { key: string; modes: Array<[string, string] | null>; kgs?: number };
  /** Refuelling connection (works landed on a pad): switch key and rate (kg/s). */
  refuel?: { key: string; kgs?: number };
  /** Tank pairs that should hold the same amount (imbalance alert above `kg`). */
  balance?: Array<{ a: string; b: string; kg: number }>;
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
 * A compartment with a `ramp` to vacuum is an airlock: opening it pumps the compartment down first.
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
  /** Obstruction sensor (ship-space box): a body inside keeps it from closing (toward 0). */
  sensor?: { min: V3; max: V3 };
}

/** Life support settings (per ship): switch keys and the compressor. The machines are parts. */
export interface LifeSupportDef {
  /** Circuit of the pressure control, heaters, fans, repress valves and the seat umbilicals. */
  circuit: SubsystemId;
  /** Pressure control selector: 0 AUTO, 1 MANUAL, 2 OFF. */
  mode: string;
  fans: string;
  heat: string;
  /** Heater draw per pressurised compartment (kW). */
  heatKw?: number;
  /** Manual repressurisation valve key of a compartment: `${repress}${compartment id}`. */
  repress?: string;
  /** Air recovery compressor: its switch and the compartment it pumps down. */
  recover?: { key: string; zone: string };
}

export interface ConsoleDef extends Frame {
  id: string;
  w: number;
  h: number;
  title: string;
  host: number;
  /** Machine it is mounted on (-1: none). */
  hostPart: number;
  /** Prop it stands on (-1: none). */
  hostProp: number;
  /** Box depth behind the face (m). */
  depth: number;
}

/** MFD page id: a key of SCREEN_PAGES (the client draws each one, see client/ship/screens.ts). */
export type ScreenPage = string;

/** Multi-function display: `pages` selectable with its bezel buttons (switch key = `id`). */
export interface ScreenDef extends Frame {
  id: string;
  w: number;
  h: number;
  pages: ScreenPage[];
  host: number;
  hostPart: number;
  /** Circuit that powers it. */
  circuit: SubsystemId;
}

/** Built-in console lamps drawn by the client (airlock: cabin side / cycling / outside side). */
export type IndicatorKind = 'reactor-core' | 'gear-greens' | 'airlock';

export interface IndicatorDef extends Frame {
  kind: IndicatorKind;
  console: string;
  /** What it shows: a part id (reactor) or a mover key (gear). Default: the first of its kind. */
  ref?: string;
}

/**
 * Double sliding door. `c` = bottom centre of the opening, `n` = opening normal (any horizontal
 * direction: a bulkhead door faces ±z, a side hatch in the hull faces ±x). The leaves slide apart
 * along the wall (up × n).
 */
export interface DoorDef {
  key: string;
  c: V3;
  n: V3;
  w: number;
  h: number;
  /** Leaves sit this far along n from the wall plane. */
  offset: number;
}

/** Horizontal axis a door's leaves slide along (up × n). */
export function doorAxis(d: Pick<DoorDef, 'n'>): V3 {
  return norm([d.n[2], 0, -d.n[0]]);
}

/** Rear ramp hinged on the floor edge; closed = vertical, open = resting on the ground. */
export interface RampDef {
  key: string;
  hinge: V3;
  w: number;
  length: number;
  t: number;
  /**
   * Hydraulic rams: `hull` = anchor in ship space, `ramp` = anchor on the ramp as
   * [x, distance along it from the hinge, depth].
   */
  pistons?: Array<{ hull: V3; ramp: V3 }>;
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
  /** Cabin light intensity (default 4.2). */
  lux?: number;
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

/**
 * In-game manual (M): sections of paragraphs and numbered steps. `[[console/key]]` in the text
 * names a control (rendered with its console and label; the tests check every reference exists).
 */
/** Diagram the manual draws for a section (the HUD owns the picture). */
/**
 * Diagram the manual draws for a section (the client owns the pictures). `plan`, `circuits`,
 * `alerts`, `pages` and `consoles` are generated from the ship itself (and live while open).
 */
export type ManualFig = 'air' | 'power' | 'cover' | 'drive' | 'leds' | 'plan' | 'circuits' | 'alerts' | 'pages' | 'consoles' | 'equipment' | 'airlock';

/** A live readout strip the manual keeps up to date while open. */
export type ManualLive = 'power' | 'air' | 'reactor' | 'fuel' | 'stores' | 'flight' | 'airlock';

export type ManualBlock =
  | string
  | { steps: string[] }
  | { note: string }
  | { warn: string }
  | { fig: ManualFig }
  | { live: ManualLive }
  /** Cards of these controls (by switch key), with their help and live state. */
  | { controls: string[] };

export interface ManualSection {
  id: string;
  title: string;
  /** One line under the title in the index. */
  lead?: string;
  body: ManualBlock[];
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
  indicators: IndicatorDef[];
  doors: DoorDef[];
  ramp?: RampDef;
  shield?: { key: string; plates: ShieldPlate[] };
  gear?: { key: string; legs: V3[] };
  zones: ZoneDef[];
  seats: SeatDef[];
  cargo: CargoDef[];
  extLights: ExtLightDef[];
  subsystems: SubsystemDef[];
  /** Switched electrical loads (lights, pumps, radar…), see LoadDef. */
  loads: LoadDef[];
  parts: PartDef[];
  props: PropDef[];
  fluid: FluidNetDef;
  compartments: CompartmentDef[];
  openings: OpeningDef[];
  movers: MoverDef[];
  life?: LifeSupportDef;
  /** Master caution switch key (latched by new alerts, reset by the crew). */
  caution: string;
  /** Annunciator lamp grid on a console (lamp names = Alert.lamp groups; the checks want a lamp per group). */
  annunciator?: { console: string; at: V2; cols: number; cell: V2; lamps: string[] };
  /**
   * System modules this ship runs (ids from modules/index.ts). Omitted: every module that finds
   * something to drive in the definition.
   */
  systems?: string[];
  defaults: Record<string, number>;
  /** Hull cross-sections (for the decor/structure builder). */
  modules: ModuleDef[];
  /** Switch keys each MFD page lists as plain rows (control name + position), by page id. */
  readouts?: Record<string, string[]>;
  /** Ship-space bounds of everything (for culling / "near the ship" tests). */
  bounds: { min: V3; max: V3 };
  manual: ManualSection[];
  /** What kind of ship it is, one line (manual, hull decal). */
  role: string;
  /** Circuits and switches the cabin / exterior lights follow. */
  lighting: LightingDef;
  /** Paint and markings. */
  livery: LiveryDef;
  /** Airlock cycle automation (modules/airlock.ts). */
  airlock?: AirlockDef;
  /** Pilot's throttle → engine thrust commands (modules/helm.ts). */
  helm?: HelmDef;
}

/** Which circuits and switches drive the lights (the view reads these, never fixed names). */
export interface LightingDef {
  /** Circuit of the cabin lights (dead → red emergency lights). */
  cabin: SubsystemId;
  /** Circuit of the exterior lights. */
  exterior: SubsystemId;
  nav: string;
  beacon: string;
  landing: string;
}

/** Linear RGB triple. */
export type RGB = [number, number, number];

/**
 * Paint: hull base colour, the stripe band and hazard accent colours, the tagline under the
 * registry, and where the registry decals go (on the nacelles, or flat on the hull sides).
 */
export interface LiveryDef {
  hull: RGB;
  stripe: RGB;
  accent: RGB;
  tagline: string;
  decals: 'nacelles' | Array<{ c: V3; n: V3; w: number; h: number }>;
}

/**
 * Airlock cycle: one selector sends the crew out or in. Out: inner door shut → air pumped back to
 * the bottles → outer door open. In: outer door shut → repressurised → inner door open.
 */
export interface AirlockDef {
  /** Compartment that cycles. */
  zone: string;
  /** Door (mover) keys. */
  inner: string;
  outer: string;
  /** Cycle switch: 0 = to the cabin side, 1 = to the outside. */
  key: string;
  /** Ventilation damper between the lock and the cabin: shut while the lock is empty. */
  duct?: string;
  /** Circuit the sequencer runs on. */
  circuit: SubsystemId;
}

/** Throttle: a multi-position switch whose fraction commands every main engine (or `engines`). */
export interface HelmDef {
  throttle: string;
  engines?: string[];
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
// Builder: hull panels
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

  /**
   * Cut a doorway out of a zone's wall: every wall panel facing `n` that the rectangle (bottom
   * centre `c`, w × h, in the wall plane) overlaps is replaced by its pieces around it (fore, aft,
   * above), so a side hatch needs no special hull section. Returns the ids of the cut panels.
   */
  cutOpening(zone: string, o: { c: V3; n: V3; w: number; h: number }): string[] {
    const along = doorAxis(o);
    const cut: string[] = [];
    const next: PanelDef[] = [];
    for (const p of this.panels) {
      const facing = p.zone === zone && p.kind !== 'floor' && Math.abs(dot(p.n, o.n)) > 0.7 && Math.abs(dot(sub(p.c, o.c), o.n)) < 0.3;
      // the full plate outline (the stored polygon is inset by half the seam gap)
      const pts = insetPoly(p.poly, -PANEL_GAP / 2).map((q) => fromFrame(p, q[0], q[1]));
      const a = pts.map((q) => dot(sub(q, o.c), along));
      const y = pts.map((q) => q[1] - o.c[1]);
      if (!facing || Math.max(...a) <= -o.w / 2 + 1e-4 || Math.min(...a) >= o.w / 2 - 1e-4 || Math.min(...y) >= o.h - 1e-4) {
        next.push(p);
        continue;
      }
      cut.push(p.id);
      const lo = madd(o.c, along, -o.w / 2);
      const hi = madd(o.c, along, o.w / 2);
      const top: V3 = [o.c[0], o.c[1] + o.h, o.c[2]];
      const pieces: Array<[string, V3[]]> = [
        ['a', clipPoly(pts, lo, along)],
        ['b', clipPoly(pts, hi, scale(along, -1))],
        ['c', clipPoly(clipPoly(clipPoly(pts, top, [0, -1, 0]), lo, scale(along, -1)), hi, along)],
      ];
      const tmp = new ShipBuilder();
      for (const [suffix, piece] of pieces) tmp.addPoly(piece, p.n, { id: `${p.id}${suffix}`, kind: p.kind, zone: p.zone, other: p.other, t: p.t, hp: p.maxHp, face: 'mid' });
      for (const piece of tmp.panels) next.push({ ...piece, paint: p.paint });
    }
    this.panels.length = 0;
    next.forEach((p, index) => this.panels.push({ ...p, index }));
    return cut;
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

// -----------------------------------------------------------------------------------------------
// Anchors: frames on the ship's surfaces. Place consoles and props by *where* they go (the left
// wall of the corridor at z, the roof above x, the front face of the battery…) instead of raw
// coordinates; `c` lies on the surface and `n` points out of it (into the room).
// -----------------------------------------------------------------------------------------------

/** Frame of a flat board facing `n` with its "up" as close to `up` as possible. */
export function boardFrame(c: V3, n: V3, up: V3 = [0, 1, 0]): Frame {
  const nn = norm(n);
  let v = sub(up, scale(nn, dot(up, nn)));
  if (len(v) < 1e-3) v = sub([0, 0, -1], scale(nn, dot([0, 0, -1], nn)));
  v = norm(v);
  return { c, u: cross(v, nn), v, n: nn };
}

/** Console-local placement → ship-space frame. */
export function onConsole(con: Frame, x: number, y: number, lift = 0): Frame {
  return { c: fromFrame(con, x, y, lift), u: con.u, v: con.v, n: con.n };
}

/** The same frame moved `d` along its normal (a board of depth d standing on a surface). */
export function standOff(f: Frame, d: number): Frame {
  return { c: madd(f.c, f.n, d), u: f.u, v: f.v, n: f.n };
}

/** The same frame slid within its plane by (du, dv). */
export function slide(f: Frame, du: number, dv: number): Frame {
  return { c: fromFrame(f, du, dv), u: f.u, v: f.v, n: f.n };
}

/** Inner surface of a module's side wall ('L' = −x) at height y and station z. Follows chamfers. */
export function wallAt(m: ModuleDef, side: 'L' | 'R', z: number, y: number): Frame {
  const P = m.profile;
  const sx = side === 'L' ? -1 : 1;
  const cy = Math.max(...P.map((p) => p[1])) / 2;
  for (let i = 0; i < P.length - 1; i++) {
    const a = P[i];
    const b = P[i + 1];
    if (((a[0] + b[0]) / 2) * sx <= 0.05 || Math.abs(b[1] - a[1]) < 1e-6) continue;
    if (y < Math.min(a[1], b[1]) - 1e-6 || y > Math.max(a[1], b[1]) + 1e-6) continue;
    const x = a[0] + ((b[0] - a[0]) * (y - a[1])) / (b[1] - a[1]);
    let n: V3 = norm([-(b[1] - a[1]), b[0] - a[0], 0]);
    if (dot(n, [-x, cy - y, 0]) < 0) n = scale(n, -1);
    return boardFrame([x, y, z], n);
  }
  throw new Error(`wallAt: no ${side} wall at y=${y} in module ${m.zone}`);
}

/** Inner surface of a module's roof above x (facing down, "up" toward the nose by default). */
export function roofAt(m: ModuleDef, x: number, z: number, up: V3 = [0, 0, -1]): Frame {
  const P = m.profile;
  for (let i = 0; i < P.length - 1; i++) {
    const a = P[i];
    const b = P[i + 1];
    if (b[0] <= a[0] || x < a[0] - 1e-6 || x > b[0] + 1e-6) continue;
    const y = a[1] + ((x - a[0]) / (b[0] - a[0])) * (b[1] - a[1]);
    let n: V3 = norm([-(b[1] - a[1]), b[0] - a[0], 0]);
    if (n[1] > 0) n = scale(n, -1);
    return boardFrame([x, y, z], n, up);
  }
  throw new Error(`roofAt: x=${x} outside module ${m.zone}`);
}

/** Top of the deck at (x, z), facing up ("up" of the board toward the nose). */
export function deckAt(x: number, z: number, up: V3 = [0, 0, -1]): Frame {
  return boardFrame([x, 0, z], [0, 1, 0], up);
}

/** Face of a bulkhead (plane z, thickness t) toward `facing` (+1 = toward the tail). */
export function bulkheadAt(z: number, t: number, x: number, y: number, facing: 1 | -1): Frame {
  return boardFrame([x, y, z + (facing * t) / 2], [0, 0, facing]);
}

export type BoxFaceId = '+x' | '-x' | '+y' | '-y' | '+z' | '-z';

/** A face of an oriented box (part / prop), `at` = offset within the face (face u, v). */
export function boxFace(b: ShipBox, face: BoxFaceId, at: V2 = [0, 0], up: V3 = [0, 1, 0]): Frame {
  const axis = face[1] === 'x' ? 0 : face[1] === 'y' ? 1 : 2;
  const s = face[0] === '+' ? 1 : -1;
  const local: V3 = [0, 0, 0];
  local[axis] = s;
  const n = rotY(local, b.yaw);
  const c: V3 = madd(b.c, n, b.half[axis]);
  const f = boardFrame(c, n, axis === 1 ? rotY([0, 0, -1], b.yaw) : up);
  return slide(f, at[0], at[1]);
}

/** Smallest distance from a point to an oriented box (0 inside). */
export function boxDistance(b: ShipBox, p: V3) {
  const l = rotY(sub(p, b.c), -b.yaw);
  const q = [Math.max(0, Math.abs(l[0]) - b.half[0]), Math.max(0, Math.abs(l[1]) - b.half[1]), Math.max(0, Math.abs(l[2]) - b.half[2])];
  return Math.hypot(q[0], q[1], q[2]);
}

/** Frame of an oriented box: u = local +X, v = up, n = local +Z, all after yaw. */
export function boxFrame(b: ShipBox): Frame {
  return { c: b.c, u: rotY([1, 0, 0], b.yaw), v: [0, 1, 0], n: rotY([0, 0, 1], b.yaw) };
}

/**
 * A point outside an interior surface. `inner.n` points into the room; the result steps back
 * through `skin` metres of plating and then `rise` metres further out (usually the part's half-height).
 */
export function outsidePoint(inner: Frame, skin: number, rise: number): V3 {
  return madd(inner.c, inner.n, -(skin + rise));
}

/** Half-width of the hull cross-section at station z and height y, on one side (+1 = starboard). */
export function hullReach(modules: ModuleDef[], z: number, y: number, side: 1 | -1): number {
  const m = modules.find((mod) => z >= mod.z0 - 1e-4 && z <= mod.z1 + 1e-4) ?? modules.reduce((a, mod) => (Math.abs((mod.z0 + mod.z1) / 2 - z) < Math.abs((a.z0 + a.z1) / 2 - z) ? mod : a));
  let best = 0;
  const P = m.profile;
  for (let i = 0; i < P.length - 1; i++) {
    const a = P[i];
    const b = P[i + 1];
    if ((a[0] + b[0]) * side < -0.05) continue;
    const y0 = Math.min(a[1], b[1]);
    const y1 = Math.max(a[1], b[1]);
    if (y < y0 - 1e-4 || y > y1 + 1e-4) continue;
    const x = Math.abs(b[1] - a[1]) < 1e-6 ? Math.max(Math.abs(a[0]), Math.abs(b[0])) : Math.abs(a[0] + ((b[0] - a[0]) * (y - a[1])) / (b[1] - a[1]));
    best = Math.max(best, x);
  }
  if (best <= 0) throw new Error(`hullReach: no wall at y=${y} z=${z}`);
  return best;
}

/** Pylon joining each moving mount (nacelle…) to the hull. Sized from the part boxes, not from hand-placed coordinates. */
export function nacellePylons(parts: PartDef[], modules: ModuleDef[]): ShipBox[] {
  const mounts = [...new Set(parts.map((p) => p.mount).filter((m): m is string => !!m))];
  const out: ShipBox[] = [];
  for (const mount of mounts) {
    const group = parts.filter((p) => p.mount === mount);
    const sx = (Math.sign(group[0].c[0]) || 1) as 1 | -1;
    const y = group.reduce((a, p) => a + p.c[1], 0) / group.length;
    const z0 = Math.min(...group.map((p) => p.c[2] - p.half[2]));
    const z1 = Math.max(...group.map((p) => p.c[2] + p.half[2]));
    const zc = (z0 + z1) / 2;
    const inner = Math.min(...group.map((p) => Math.abs(p.c[0]) - p.half[0]));
    const wall = hullReach(modules, zc, Math.min(Math.max(y, 0.2), 2.2), sx);
    const gap = inner - wall;
    if (gap < 0.02) continue;
    out.push({ c: [sx * (wall + inner) * 0.5, y, zc], half: [gap / 2, 0.16, Math.min(1.4, (z1 - z0) * 0.2)], yaw: 0 });
  }
  return out;
}

/**
 * Flip-cover hit box, matching the lid mesh: a thin plate on the face, hinged at its top edge.
 * Open, it swings out along +n and leaves the switch underneath clear. Half extents are the plate,
 * not the generous box used for bare switches.
 */
export function coverHit(c: Frame, open: boolean): { frame: Frame; half: V3 } {
  const ang = open ? -1.65 : 0;
  const cos = Math.cos(ang);
  const sin = Math.sin(ang);
  // same composition as the lid instance: hinge, spin, then seat the plate on the face
  const ly = -0.04;
  const lz = 0.012;
  const cy = ly * cos - lz * sin + 0.04;
  const cz = ly * sin + lz * cos;
  const v = madd(scale(c.v, cos), c.n, sin);
  const n = madd(scale(c.n, cos), c.v, -sin);
  return { frame: { c: madd(madd(c.c, c.v, cy), c.n, cz), u: c.u, v, n }, half: [0.036, 0.04, 0.01] };
}

/** Covers that are still shut, keyed by their switch (the `guard` of the control underneath). */
export function shutCovers(controls: Array<{ kind: ControlKind; key: string }>, sw: Record<string, number>): Set<string> {
  const shut = new Set<string>();
  for (const c of controls) if (c.kind === 'cover' && (sw[c.key] ?? 0) !== 1) shut.add(c.key);
  return shut;
}

/**
 * What a crosshair can land on. A shut cover hides the control it guards; the cover itself is a
 * plate, bare switches keep a generous box so they are easy to aim at.
 */
export function controlHit(c: ControlDef, sw: Record<string, number>, shut: ReadonlySet<string>): { frame: Frame; half: V3 } | null {
  if (c.guard && shut.has(c.guard)) return null;
  if (c.kind === 'cover') return coverHit(c, (sw[c.key] ?? 0) === 1);
  // a guarded switch sits under a plate: keep its box on the face so the open lid stays clickable
  if (c.guard) return { frame: { c: madd(c.c, c.n, 0.016), u: c.u, v: c.v, n: c.n }, half: [c.half[0] + 0.006, c.half[1] + 0.006, 0.016] };
  return { frame: { c: madd(c.c, c.n, c.half[2] * 0.5), u: c.u, v: c.v, n: c.n }, half: [c.half[0] * 1.5 + 0.01, c.half[1] * 1.5 + 0.01, c.half[2] + 0.02] };
}

/** Which page a display is showing. The bezel buttons write this index into `sw[screen.id]`. */
export function activePage(screen: Pick<ScreenDef, 'id' | 'pages'>, sw: Record<string, number>): ScreenPage {
  const i = Math.round(sw[screen.id] ?? 0);
  return screen.pages[Math.max(0, Math.min(screen.pages.length - 1, i))] ?? screen.pages[0];
}

// -----------------------------------------------------------------------------------------------
// Builder: consoles and their controls
// -----------------------------------------------------------------------------------------------

export interface ControlSpec {
  key: string;
  kind: ControlKind;
  label: string;
  name: string;
  /** What it does (helmet HUD + manual). Default: its name and positions. */
  help?: string;
  states?: string[];
  /** Position on the board (m from its centre, u right / v up). */
  at: V2;
  requires?: SubsystemId;
  action?: ControlAction;
  value?: number;
  /** Flip cover over the control (a separate clickable cover is generated). */
  guard?: string;
  /** `cycle` knobs: selectors wrap past the last position (default); scales (power, range) pass false. */
  wrap?: boolean;
}

export interface ConsoleSpec {
  id: string;
  title: string;
  /** Surface the board is mounted on (see wallAt / roofAt / boxFace…): the face stands `depth` off it. */
  on?: Frame;
  /** Explicit face frame (tilted desks). */
  frame?: Frame;
  w: number;
  h: number;
  depth: number;
  /** Mounted on a machine (part id): its controls are lost with it. */
  part?: string;
  /** Stands on a prop (prop id) instead of a hull panel. */
  prop?: string;
  /** Free-standing: not mounted on a hull panel (survives when the panel behind it is blown out). */
  free?: boolean;
  controls: ControlSpec[];
  screens?: Array<{ id: string; pages: ScreenPage[]; at: V2; w: number; h: number; circuit?: SubsystemId }>;
  indicators?: Array<{ kind: IndicatorKind; at: V2; ref?: string }>;
}

/** Hit box half extents (u, v, n) per control kind. */
export const CONTROL_HALF: Record<ControlKind, V3> = {
  button: [0.035, 0.035, 0.03],
  toggle: [0.03, 0.045, 0.05],
  lever: [0.04, 0.09, 0.08],
  breaker: [0.04, 0.06, 0.04],
  master: [0.05, 0.04, 0.03],
  mushroom: [0.045, 0.045, 0.05],
  rotary: [0.035, 0.035, 0.04],
  cover: [0.036, 0.04, 0.012],
  valve: [0.06, 0.06, 0.05],
  bezel: [0.03, 0.014, 0.02],
};

/**
 * MFD pages: bezel label and one line of help. A new page is an entry here plus its drawing in
 * client/ship/screens.ts (pages without a drawing show a placeholder).
 */
export const SCREEN_PAGES: Record<string, { label: string; help: string }> = {
  status: { label: 'SIST', help: 'resumen de todos los sistemas' },
  hull: { label: 'CASCO', help: 'integridad de cada panel del casco' },
  power: { label: 'ENERG', help: 'fuentes, batería y cada circuito' },
  fuel: { label: 'COMB', help: 'depósitos, válvulas y alimentación' },
  atmos: { label: 'ATMOS', help: 'presión y aire de cada compartimento' },
  engines: { label: 'MOTOR', help: 'motores, APU y RCS' },
  reactor: { label: 'REACT', help: 'núcleo, refrigerante y radiadores' },
  alerts: { label: 'ALARM', help: 'lista de alarmas activas' },
  flight: { label: 'VUELO', help: 'estado de vuelo' },
  nav: { label: 'NAV', help: 'mapa de la zona' },
  radar: { label: 'RADAR', help: 'contactos del radar' },
  weapons: { label: 'ARMAS', help: 'torreta' },
  stores: { label: 'VÍVER', help: 'agua, víveres, reciclador y paneles solares' },
  lock: { label: 'ESCL', help: 'ciclo de la esclusa, presión y puertas' },
  mass: { label: 'MASA', help: 'masa, centro de masas, empuje y Δv' },
};

/** Bezel label of a page (unknown pages show their id). */
export const pageLabel = (page: ScreenPage) => SCREEN_PAGES[page]?.label ?? page.toUpperCase().slice(0, 5);

/** Default help line of a control: its name, its positions and the cover warning. */
function defaultHelp(c: ControlSpec): string {
  const states = (c.states ?? ['APAGADO', 'ENCENDIDO']).filter((s) => s).join(' / ');
  return `${c.name}. ${states}.${c.guard ? ' Tiene tapa de seguridad: ábrela antes.' : ''}`;
}

/** A console face frame from its spec (explicit, or standing on its surface anchor). */
export function consoleFace(spec: ConsoleSpec): Frame {
  if (spec.frame) return spec.frame;
  if (spec.on) return standOff(spec.on, spec.depth);
  throw new Error(`console ${spec.id}: needs 'on' or 'frame'`);
}

/**
 * Turn console specs into consoles, controls (with generated flip covers and MFD page buttons),
 * screens and indicators. A console mounted on a part or prop says so; otherwise it is hosted by
 * the hull panel right behind it (and dies with it).
 */
export function buildConsoles(specs: ConsoleSpec[], panels: PanelDef[], parts: Array<{ id: string }>, props: Array<{ id: string }>) {
  const consoles: ConsoleDef[] = [];
  const controls: ControlDef[] = [];
  const screens: ScreenDef[] = [];
  const indicators: IndicatorDef[] = [];
  const indexOf = (list: Array<{ id: string }>, id: string | undefined, what: string, con: string) => {
    if (!id) return -1;
    const i = list.findIndex((x) => x.id === id);
    if (i < 0) throw new Error(`console ${con}: unknown ${what} "${id}"`);
    return i;
  };
  for (const spec of specs) {
    const f = consoleFace(spec);
    const hostPart = indexOf(parts, spec.part, 'part', spec.id);
    const hostProp = indexOf(props, spec.prop, 'prop', spec.id);
    const back: V3 = madd(f.c, f.n, -(spec.depth + 0.04));
    const host = spec.free || hostPart >= 0 || hostProp >= 0 ? -1 : hostPanel(panels, back, 0.25);
    consoles.push({ ...f, id: spec.id, w: spec.w, h: spec.h, title: spec.title, host, hostPart, hostProp, depth: spec.depth });
    const add = (c: ControlSpec) => {
      const id = `${spec.id}/${c.key}${c.action === 'set' ? `=${c.value}` : ''}`;
      if (controls.some((x) => x.id === id)) throw new Error(`control ${id} defined twice`);
      controls.push({
        ...onConsole(f, c.at[0], c.at[1]),
        index: controls.length,
        id,
        key: c.key,
        action: c.action ?? 'toggle',
        value: c.value,
        kind: c.kind,
        label: c.label,
        name: c.name,
        help: c.help ?? defaultHelp(c),
        states: c.states ?? ['APAGADO', 'ENCENDIDO'],
        wrap: c.wrap ?? true,
        requires: c.requires,
        guard: c.guard,
        console: spec.id,
        host,
        hostPart,
        half: CONTROL_HALF[c.kind],
      });
    };
    for (const s of spec.screens ?? []) {
      screens.push({ ...onConsole(f, s.at[0], s.at[1], 0.004), id: s.id, w: s.w, h: s.h, pages: s.pages, host, hostPart, circuit: s.circuit ?? 'avionics' });
      // bezel buttons under the display select its page
      s.pages.forEach((pg, i) => {
        const x = s.at[0] - s.w / 2 + ((i + 0.5) * s.w) / s.pages.length;
        const label = pageLabel(pg);
        const help = SCREEN_PAGES[pg]?.help;
        add({ key: s.id, kind: 'bezel', label, name: `Pantalla · ${label}`, help: `Muestra la página ${label}${help ? `: ${help}` : ''}.`, states: s.pages.map(pageLabel), at: [x, s.at[1] - s.h / 2 - 0.028], action: 'set', value: i });
      });
    }
    for (const c of spec.controls) {
      // flip cover in front of a guarded control
      if (c.guard) add({ key: c.guard, kind: 'cover', label: '', name: `${c.name} · tapa de seguridad`, help: `Tapa de seguridad: ábrela para poder accionar «${c.name}».`, states: ['CERRADA', 'ABIERTA'], at: c.at });
      add(c);
    }
    for (const ind of spec.indicators ?? []) indicators.push({ ...onConsole(f, ind.at[0], ind.at[1], 0.004), kind: ind.kind, console: spec.id, ref: ind.ref });
  }
  return { consoles, controls, screens, indicators };
}

/**
 * Parts / props from compact specs (defaults filled in). Most ships build their parts from the
 * component catalog instead (`part('reactor.fission.XS', { id, c, zone, circuit })`, see
 * catalog/index.ts), which fills type, size, box, integrity, mass, parameters and model.
 */
export type PartSpec = Omit<PartDef, 'index' | 'yaw' | 'soft' | 'p' | 'model' | 'shape' | 'mass' | 'look'> & Partial<Pick<PartDef, 'yaw' | 'soft' | 'p' | 'model' | 'shape' | 'mass' | 'look'>>;
export type PropSpec = Omit<PropDef, 'index' | 'yaw' | 'p' | 'collide' | 'zone' | 'mass' | 'look'> & Partial<Pick<PropDef, 'yaw' | 'p' | 'collide' | 'zone' | 'mass' | 'look'>>;

/** Mass of a box of stuff at a given bulk density (kg/m³). */
export const boxMass = (half: V3, density: number) => 8 * half[0] * half[1] * half[2] * density;

export function buildParts(specs: PartSpec[]): PartDef[] {
  return specs.map((p, index) => ({ ...p, index, yaw: p.yaw ?? 0, soft: p.soft ?? 1, p: p.p ?? {}, model: p.model ?? p.type, shape: p.shape ?? 'box', mass: p.mass ?? boxMass(p.half, 600), look: p.look ?? {} }));
}

export function buildProps(specs: PropSpec[]): PropDef[] {
  return specs.map((p, index) => ({ ...p, index, yaw: p.yaw ?? 0, p: p.p ?? {}, collide: p.collide ?? 'box', zone: p.zone ?? null, look: p.look ?? {}, mass: p.mass ?? boxMass(p.half, 150) }));
}

/**
 * Paint every hull panel by rules (first match wins): livery bands are data of each ship
 * (e.g. lower wall rows dark, a stripe row, hazard stripes around the ramp).
 */
export function paintPanels(panels: PanelDef[], rules: Array<{ scheme: number; when: (p: PanelDef) => boolean }>) {
  for (const p of panels) {
    if (p.kind !== 'hull') continue;
    p.paint = rules.find((r) => r.when(p))?.scheme ?? 0;
  }
  return panels;
}

/** Block from the deck up to just under a console face, so the board sits on something instead of floating. */
export function supportBlock(id: string, con: { c: V3; n: V3; w: number; h: number; depth: number }, zone: string): PropSpec {
  const top = Math.max(0.08, con.c[1] - con.h * 0.42);
  const hh = top / 2;
  return {
    id,
    model: 'block',
    c: [con.c[0] - con.n[0] * con.depth, hh, con.c[2] - con.n[2] * con.depth],
    half: [Math.max(0.12, con.w * 0.48), hh, Math.max(0.14, con.depth + 0.22)],
    zone,
    collide: 'box',
  };
}

/**
 * In-game manual: the intro sections, then one chapter per console listing every control as
 * `[[console/key]]` (the HUD resolves that to the control's name; tests check each id exists).
 */
export function shipManual(intro: ManualSection[]): ManualSection[] {
  // the console-by-console reference is generated by the client from the controls themselves
  return intro;
}

/** Controls laid out on a grid: `cols` per row from `origin` (top-left cell centre), cell pitch (dx, dy). */
export function grid<T extends Omit<ControlSpec, 'at'>>(items: T[], o: { cols: number; origin: V2; dx: number; dy: number }): Array<T & { at: V2 }> {
  return items.map((c, i) => ({ ...c, at: [o.origin[0] + (i % o.cols) * o.dx, o.origin[1] - Math.floor(i / o.cols) * o.dy] as V2 }));
}

// -----------------------------------------------------------------------------------------------
// Movers: obstruction sensors
// -----------------------------------------------------------------------------------------------

/** Obstruction sensor of a sliding door: the doorway plus a margin (a body there holds it open). */
export function doorSensor(d: DoorDef): { min: V3; max: V3 } {
  const across = Math.abs(d.n[0]) > Math.abs(d.n[2]) ? 2 : 0; // the leaves slide along this axis
  const along = across === 0 ? 2 : 0;
  const min: V3 = [0, -0.5, 0];
  const max: V3 = [0, d.h, 0];
  min[across] = d.c[across] - d.w / 2 - 0.3;
  max[across] = d.c[across] + d.w / 2 + 0.3;
  min[along] = d.c[along] - 0.55;
  max[along] = d.c[along] + 0.55;
  return { min, max };
}

/** Obstruction sensor of a rear ramp: where it swings (someone standing there stops it closing). */
export function rampSensor(r: RampDef): { min: V3; max: V3 } {
  return { min: [r.hinge[0] - r.w / 2 - 0.3, -10, r.hinge[2] - 0.45], max: [r.hinge[0] + r.w / 2 + 0.3, 0.6, r.hinge[2] + r.length + 0.4] };
}

// -----------------------------------------------------------------------------------------------
// Whole ship: defaults, derived data and structural checks
// -----------------------------------------------------------------------------------------------

/** What a ship file has to give; everything else defaults to "none". */
export type ShipSpec = Pick<ShipDef, 'id' | 'name' | 'registry' | 'floorHeight' | 'panels' | 'modules' | 'bounds'> &
  Partial<Omit<ShipDef, 'id' | 'name' | 'registry' | 'floorHeight' | 'panels' | 'modules' | 'bounds' | 'lighting' | 'livery'>> & {
    lighting?: Partial<LightingDef>;
    livery?: Partial<LiveryDef>;
  };

/** Light keys a ship gets unless it names its own. */
export const DEFAULT_LIGHTING: LightingDef = { cabin: 'lights', exterior: 'ext', nav: 'light.nav', beacon: 'light.beacon', landing: 'light.landing' };
/** Light circuits when the ship names none: 'lights' / 'ext' if it has them, else its first circuit. */
function lightingCircuits(subs: SubsystemDef[]): Partial<LightingDef> {
  const pick = (id: string) => (subs.some((c) => c.id === id) ? id : subs[0]?.id ?? id);
  return { cabin: pick('lights'), exterior: pick('ext') };
}

/** Default paint: light grey hull, rust stripe, amber hazard. */
export const DEFAULT_LIVERY: LiveryDef = { hull: [0.3, 0.305, 0.31], stripe: [0.46, 0.15, 0.02], accent: [0.5, 0.26, 0.02], tagline: '', decals: 'nacelles' };

/**
 * Complete a ship definition: empty lists for what it doesn't have, conduits routed behind the
 * panels, breakers closed and priorities NORMAL unless the defaults say otherwise. Throws on
 * references to things that don't exist (a typo in data must not become a silent no-op).
 */
export function finishShip(spec: ShipSpec): ShipDef {
  const def: ShipDef = {
    controls: [],
    consoles: [],
    screens: [],
    indicators: [],
    doors: [],
    zones: [],
    seats: [],
    cargo: [],
    extLights: [],
    subsystems: [],
    loads: [],
    parts: [],
    props: [],
    fluid: { manifolds: [], pipes: [] },
    compartments: [],
    openings: [],
    movers: [],
    caution: 'caution',
    manual: [],
    role: '',
    ...spec,
    lighting: { ...DEFAULT_LIGHTING, ...lightingCircuits(spec.subsystems ?? []), ...(spec.lighting ?? {}) },
    livery: { ...DEFAULT_LIVERY, ...(spec.livery ?? {}) },
    defaults: { ...(spec.defaults ?? {}) },
  };
  routeConduits(def.panels, def.subsystems);
  for (const c of def.subsystems) {
    def.defaults[c.breaker] ??= 1;
    def.defaults[c.priority] ??= 1;
  }
  def.defaults[def.caution] ??= 0;
  const problems = checkShip(def);
  if (problems.length) throw new Error(`ship ${def.id}:\n  ${problems.join('\n  ')}`);
  return def;
}

/** Broken references in a definition (empty = consistent). */
export function checkShip(def: ShipDef): string[] {
  const out: string[] = [];
  const circuits = new Set(def.subsystems.map((c) => c.id));
  const parts = new Set(def.parts.map((p) => p.id));
  const comps = new Set(def.compartments.map((c) => c.id));
  const circuit = (c: string | undefined, where: string) => {
    if (c !== undefined && !circuits.has(c)) out.push(`${where}: circuito "${c}" no existe`);
  };
  const dup = (ids: string[], what: string) => {
    const seen = new Set<string>();
    for (const id of ids) {
      if (seen.has(id)) out.push(`${what} "${id}" repetido`);
      seen.add(id);
    }
  };
  dup(def.parts.map((p) => p.id), 'máquina');
  dup(def.subsystems.map((c) => c.id), 'circuito');
  dup(def.compartments.map((c) => c.id), 'compartimento');
  dup(def.movers.map((m) => m.key), 'mecanismo');
  for (const c of def.controls) circuit(c.requires, `mando ${c.id}`);
  for (const s of def.screens) {
    circuit(s.circuit, `pantalla ${s.id}`);
    for (const pg of s.pages) if (!SCREEN_PAGES[pg]) out.push(`pantalla ${s.id}: página "${pg}" no registrada en SCREEN_PAGES`);
  }
  for (const l of def.loads) {
    circuit(l.circuit, `carga ${l.key}`);
    if (l.part && !parts.has(l.part)) out.push(`carga ${l.key}: máquina "${l.part}" no existe`);
  }
  for (const m of def.movers) circuit(m.circuit, `mecanismo ${m.key}`);
  for (const p of def.parts) {
    circuit(p.circuit, `máquina ${p.id}`);
    if (p.zone !== null && !def.zones.some((z) => z.id === p.zone)) out.push(`máquina ${p.id}: zona "${p.zone}" no existe`);
    if (p.link && !parts.has(p.link)) out.push(`máquina ${p.id}: enlazada a "${p.link}", que no existe`);
  }
  const nodes = new Set([...def.fluid.manifolds, ...parts]);
  for (const pipe of def.fluid.pipes) for (const n of [pipe.a, pipe.b]) if (!nodes.has(n)) out.push(`tubería ${pipe.a}–${pipe.b}: nodo "${n}" no existe`);
  for (const p of def.parts) if (p.feed && !nodes.has(p.feed)) out.push(`máquina ${p.id}: alimentación "${p.feed}" no existe`);
  circuit(def.fluid.circuit, 'red de propelente');
  for (const mode of def.fluid.transfer?.modes ?? []) for (const t of mode ?? []) if (!parts.has(t)) out.push(`transferencia: depósito "${t}" no existe`);
  for (const b of def.fluid.balance ?? []) for (const t of [b.a, b.b]) if (!parts.has(t)) out.push(`equilibrio: depósito "${t}" no existe`);
  for (const o of def.openings) for (const z of [o.a, o.b]) if (z !== null && !comps.has(z)) out.push(`abertura ${o.id}: compartimento "${z}" no existe`);
  for (const c of def.compartments) if (!def.zones.some((z) => z.id === c.id)) out.push(`compartimento ${c.id}: no hay zona con ese id (la tripulación no sabría que está dentro)`);
  for (const p of def.panels) if (!def.zones.some((z) => z.id === p.zone) && comps.size) out.push(`panel ${p.id}: zona "${p.zone}" no existe`);
  if (def.life) {
    circuit(def.life.circuit, 'soporte vital');
    if (def.life.recover && !comps.has(def.life.recover.zone)) out.push(`compresor: compartimento "${def.life.recover.zone}" no existe`);
  }
  for (const ind of def.indicators) if (!def.consoles.some((c) => c.id === ind.console)) out.push(`indicador ${ind.kind}: consola "${ind.console}" no existe`);
  if (def.annunciator && !def.consoles.some((c) => c.id === def.annunciator!.console)) out.push(`anunciador: consola "${def.annunciator.console}" no existe`);
  if (def.subsystems.length) {
    circuit(def.lighting.cabin, 'luces de cabina');
    circuit(def.lighting.exterior, 'luces exteriores');
  }
  const movers = new Set(def.movers.map((m) => m.key));
  for (const d of def.doors) {
    if (Math.abs(d.n[1]) > 1e-6) out.push(`puerta ${d.key}: la normal tiene que ser horizontal`);
    if (!movers.has(d.key)) out.push(`puerta ${d.key}: no hay mecanismo con esa tecla`);
  }
  if (def.airlock) {
    const a = def.airlock;
    if (!comps.has(a.zone)) out.push(`esclusa: compartimento "${a.zone}" no existe`);
    for (const k of [a.inner, a.outer]) if (!def.openings.some((o) => o.key === k && o.kind === 'door')) out.push(`esclusa: "${k}" no es una puerta de la nave`);
    circuit(a.circuit, 'esclusa');
    if (!def.life?.recover || def.life.recover.zone !== a.zone) out.push('esclusa: el compresor de recuperación tiene que vaciar la esclusa (life.recover.zone)');
  }
  if (def.helm) {
    if (!def.controls.some((c) => c.key === def.helm!.throttle)) out.push(`acelerador: ningún mando mueve "${def.helm.throttle}"`);
    for (const e of def.helm.engines ?? []) if (!def.parts.some((p) => p.id === e && p.type === 'engine')) out.push(`acelerador: "${e}" no es un motor`);
  }
  for (const s of def.seats) if (!zoneAtPoint(def.zones, [s.root[0], s.root[1] + 1, s.root[2]])) out.push(`asiento ${s.id}: fuera de cualquier zona`);
  return out;
}

/** Zone containing a ship-space point (same rule as the crew uses, see crew.ts). */
export function zoneAtPoint(zones: ZoneDef[], l: V3): ZoneDef | null {
  return zones.find((z) => l[0] >= z.min[0] && l[0] <= z.max[0] && l[1] >= z.min[1] - 0.2 && l[1] <= z.max[1] && l[2] >= z.min[2] && l[2] <= z.max[2]) ?? null;
}
