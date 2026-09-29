// Component catalog: standard machines and furniture every ship builds from. A component is the
// part of a machine that does not depend on the ship — its type, size class, box, integrity,
// armour, mass, the parameters its system module reads (`p`) and how its model looks. A ship
// only says where it goes and what it is wired to:
//
//   part('reactor.fission.XS', { id: 'reactor', c: [0, 1, 3], zone: 'eng', circuit: 'cool', tag: 'rx' })
//
// Anything can still be overridden per ship (name, p, half…), and a ship can define its own
// components on top of a standard one (`defineComponent({ base: 'reactor.fission.S', … })`).
// See docs/SHIPS.md §5.

import { boxMass, type PartSpec, type PartType, type PropSpec, type Size } from '../def.js';
import type { V3 } from '../geom.js';

export interface ComponentDef {
  /** Catalog id: family.variant.size, e.g. 'reactor.fission.M'. */
  id: string;
  type: PartType;
  size: Size;
  /** Manufacturer id (makers.ts): trim colour and the maker's name on the sheet. */
  maker: string;
  /** Default HUD name (a ship usually names it its own way). */
  name: string;
  /** One line: what it is (manual equipment table, catalog sheet). */
  desc: string;
  /** Box half extents (m). */
  half: V3;
  maxHp: number;
  /** Blast damage multiplier (armour < 1). Default 1. */
  soft?: number;
  /**
   * Share of its integrity a violent decompression tears off (boiling liquids, burst seals and
   * cells, loose things flung into it): 0 sealed and rugged … 1 wrecked. Default `DECOMP_DEFAULT`.
   */
  decomp?: number;
  /** Dry mass (kg). */
  mass: number;
  /** Parameters its system module reads (kw, kwh, cap, thrustN…). */
  p: Record<string, number>;
  /** Model builder (client/ship/models); default: the type. */
  model?: string;
  /** Model parameters (variant, fins, bottles…). */
  look?: Record<string, number>;
  shape?: 'box' | 'cylZ' | 'none';
  /** Annunciator lamp for its alerts. */
  lamp?: string;
  /** Its own voice per sound role (`{ run: 'mach.pump' }`, see PartDef.sounds). */
  sounds?: Record<string, string>;
}

/** Furniture / structure (props): a model, a default box and a mass. */
export interface FurnitureDef {
  id: string;
  model: string;
  name: string;
  half: V3;
  mass: number;
  collide: 'box' | 'hull' | 'none';
  look?: Record<string, number>;
}

const COMPONENTS = new Map<string, ComponentDef>();
const FURNITURE = new Map<string, FurnitureDef>();

export function registerComponents(list: ComponentDef[]) {
  for (const c of list) {
    if (COMPONENTS.has(c.id)) throw new Error(`componente ${c.id} repetido en el catálogo`);
    COMPONENTS.set(c.id, c);
  }
}

export function registerFurniture(list: FurnitureDef[]) {
  for (const f of list) {
    if (FURNITURE.has(f.id)) throw new Error(`mueble ${f.id} repetido en el catálogo`);
    FURNITURE.set(f.id, f);
  }
}

/** A catalog component by id (throws on typos: a silent default would build the wrong machine). */
export function component(id: string): ComponentDef {
  const c = COMPONENTS.get(id);
  if (!c) throw new Error(`componente "${id}" no está en el catálogo`);
  return c;
}

export function furniture(id: string): FurnitureDef {
  const f = FURNITURE.get(id);
  if (!f) throw new Error(`mueble "${id}" no está en el catálogo`);
  return f;
}

/** Every registered component / furniture item (catalog tools, tests). */
export const allComponents = () => [...COMPONENTS.values()];
export const allFurniture = () => [...FURNITURE.values()];

/**
 * A ship's own component, built on a catalog one: only what differs. Registered under its id so
 * several parts (and other ships) can use it.
 */
export function defineComponent(spec: Partial<ComponentDef> & { id: string; base: string }): ComponentDef {
  const { base, ...rest } = spec;
  const b = component(base);
  const c: ComponentDef = { ...b, ...rest, p: { ...b.p, ...(rest.p ?? {}) }, look: { ...(b.look ?? {}), ...(rest.look ?? {}) } };
  registerComponents([c]);
  return c;
}

/** Where a part goes and what it is wired to; anything of the component may be overridden. */
export type Placement = Omit<PartSpec, 'type' | 'half' | 'maxHp' | 'name'> & Partial<Pick<PartSpec, 'half' | 'maxHp' | 'name'>>;

/**
 * A part spec from a catalog component. `p` and `look` merge over the component's; a changed box
 * scales the mass with the volume unless `mass` is given.
 */
export function part(id: string | ComponentDef, at: Placement): PartSpec {
  const c = typeof id === 'string' ? component(id) : id;
  const half = at.half ?? c.half;
  const vol = (h: V3) => h[0] * h[1] * h[2];
  return {
    ...at,
    type: c.type,
    name: at.name ?? c.name,
    half,
    maxHp: at.maxHp ?? c.maxHp,
    soft: at.soft ?? c.soft,
    decomp: at.decomp ?? c.decomp,
    p: { ...c.p, ...(at.p ?? {}) },
    model: at.model ?? c.model,
    look: { ...(c.look ?? {}), ...(at.look ?? {}) },
    shape: at.shape ?? c.shape,
    lamp: at.lamp ?? c.lamp,
    mass: at.mass ?? c.mass * (vol(half) / vol(c.half)),
    component: c.id,
    size: c.size,
    maker: at.maker ?? c.maker,
    sounds: c.sounds || at.sounds ? { ...(c.sounds ?? {}), ...(at.sounds ?? {}) } : undefined,
  };
}

/** A prop from a furniture item: position (and optionally its own box, yaw, look). */
export function prop(id: string, at: Omit<PropSpec, 'model' | 'half'> & Partial<Pick<PropSpec, 'half' | 'model'>>): PropSpec {
  const f = furniture(id);
  const half = at.half ?? f.half;
  return {
    ...at,
    model: at.model ?? f.model,
    half,
    collide: at.collide ?? f.collide,
    look: { ...(f.look ?? {}), ...(at.look ?? {}) },
    mass: at.mass ?? (at.half ? boxMass(half, f.mass / (8 * f.half[0] * f.half[1] * f.half[2])) : f.mass),
  };
}

// -----------------------------------------------------------------------------------------------
// Data sheet: the numbers worth reading, from the parameters (manual, catalog tool)
// -----------------------------------------------------------------------------------------------

/** Machine types that make power (their `kw` is output, not draw). */
const SOURCES = new Set(['reactor', 'apu', 'solar']);

/** Label, unit and scale of every parameter shown on a sheet (others stay internal). */
const FIELDS: Record<string, { label: string; unit: string; k?: number; digits?: number }> = {
  kwh: { label: 'Capacidad', unit: 'kWh' },
  maxOut: { label: 'Descarga máx.', unit: 'kW' },
  heatKw: { label: 'Calor', unit: 'kW' },
  thrustN: { label: 'Empuje', unit: 'kN', k: 1 / 1000, digits: 1 },
  flowKg: { label: 'Caudal', unit: 'kg/s', digits: 2 },
  cap: { label: 'Capacidad', unit: 'kg' },
  deployed: { label: 'Área desplegada', unit: 'm²' },
  stowed: { label: 'Área plegada', unit: 'm²' },
  rate: { label: 'Rendimiento', unit: '×', digits: 2 },
  eff: { label: 'Recuperación', unit: '%', k: 100 },
  kwActive: { label: 'Consumo activo', unit: 'kW', digits: 1 },
  startS: { label: 'Arranque', unit: 's' },
};

export function componentSheet(c: Pick<ComponentDef, 'type' | 'p' | 'mass' | 'maxHp'>): Array<[string, string]> {
  const out: Array<[string, string]> = [];
  if (c.p.kw !== undefined) out.push([SOURCES.has(c.type) ? 'Potencia' : 'Consumo', `${fmt(c.p.kw, 1)} kW`]);
  for (const [k, f] of Object.entries(FIELDS)) {
    const v = c.p[k];
    if (v === undefined) continue;
    if (k === 'stowed' && c.p.deployed === v) continue;
    out.push([k === 'deployed' && c.p.stowed === v ? 'Área' : f.label, `${fmt(v * (f.k ?? 1), f.digits ?? 0)} ${f.unit}`]);
  }
  if (c.p.thrustN && c.p.flowKg) out.push(['Impulso específico', `${Math.round(c.p.thrustN / (c.p.flowKg * 9.81))} s`]);
  out.push(['Masa', `${Math.round(c.mass)} kg`]);
  out.push(['Integridad', `${c.maxHp}`]);
  return out;
}

const fmt = (v: number, digits: number) => (Math.abs(v - Math.round(v)) < 1e-9 && digits <= 1 ? String(Math.round(v)) : v.toFixed(digits)).replace('.', ',');
