/**
 * Fast ship check: the rules, without a browser. Prints a percent before each job and exits 1
 * on the first failed assertion inside a job (the rest still run).
 *
 *   npm run test:ship
 */
import { activePage, buildParts, checkShip, controlHit, coverHit, finishShip, hatchPlate, nacellePylons, ShipBuilder, shutCovers, zoneAtPoint, type ManualSection } from '../../src/shared/ship/def.js';
import { crewStep } from '../../src/shared/ship/crew.js';
import type { SystemFactory } from '../../src/shared/ship/modules/api.js';
import { ENG } from '../../src/shared/ship/modules/apu.js';
import { SYSTEM_FACTORIES } from '../../src/shared/ship/modules/index.js';
import { ReasonHold } from '../../src/shared/ship/hold.js';
import { cross, madd, qConj, qRotate, rayBox, scale, type V3 } from '../../src/shared/ship/geom.js';
import { ALBATROS, HAULER, PEREGRINA, SHIP_DEFS } from '../../src/shared/ship/ships/index.js';
import { allComponents, allFurniture, component, componentSheet, defineComponent, part, PROFILES } from '../../src/shared/ship/catalog/index.js';
import {
  carried,
  clonePose,
  FLIGHT,
  FLIGHT_IDLE,
  hermitePose,
  integrateBody,
  lerpPose,
  levelness,
  massProperties,
  mulM3,
  NAV_POINTS,
  bearingTo,
  performance,
  poseYaw,
  tensor,
  toWorld,
  type FlightCommand,
  type ShipPose,
} from '../../src/shared/ship/flight/index.js';
import { LOCK } from '../../src/shared/ship/modules/airlock.js';
import { qYaw } from '../../src/shared/ship/geom.js';
import type { ShipDef } from '../../src/shared/ship/def.js';
import { ShipSim, placeShip, SYSTEMS_HZ } from '../../src/shared/ship/sim.js';
import { RX } from '../../src/shared/ship/modules/reactor.js';
import { altitudeOf, heightAboveGround, MOON_BODY, orbitOf, surfaceOf } from '../../src/shared/space/body.js';
import { BodySurface } from '../../src/shared/space/surface.js';
import { SurfaceGround } from '../../src/shared/space/tangent.js';
import { BOOST, type Engine } from '../../src/shared/ship/modules/engines.js';
import { airAccel, airPush, boxDrag, DRAG } from '../../src/shared/ship/airflow.js';

/**
 * Test grounds: the Moon's body with a surface of its own (space/surface.ts), `relief` m of gentle
 * swell (0: flat, the mean sphere) with its reference `datum` m over the mean radius. The ships are
 * placed at the world origin, where the frame of the body's tangent plane is the world's axes.
 */
function testSurface(relief: number, datum = 0) {
  return new BodySurface({ seed: 1, datum, relief: { amp: relief, wavelength: 110, octaves: relief ? 2 : 0, gain: 0.5, lacunarity: 2.3 }, highlands: { amp: 0, wavelength: 1e6 }, craters: [], complexFrom: 7000, albedo: { highland: 1, mare: 1 } }, MOON_BODY);
}
const ground = testSurface(0);
let failed = 0;

const SHIPS: ShipDef[] = Object.values(SHIP_DEFS);

/** The ground of a test surface seen from its tangent frame at the world origin. */
const at = (on: BodySurface) => new SurfaceGround(MOON_BODY, on).layDir([0, 1, 0]);
/** Height over a test surface of a world point. */
const alt = (on: BodySurface) => (p: readonly number[]) => heightAboveGround(MOON_BODY, p, on);

function sim(def: ShipDef, yaw = 0, on: BodySurface = ground) {
  return new ShipSim(1, def, placeShip(def, at(on), 0, 0, yaw), alt(on));
}

function fresh() {
  return sim(HAULER);
}

/** Every door and the ramp shut, every compartment at cabin pressure, settled for half a second. */
function sealed(def: ShipDef) {
  const s = sim(def);
  for (const o of def.openings) {
    const i = s.sys.moverIndex(o.key);
    if (i === undefined) continue;
    s.sw[o.key] = 0;
    s.st[i] = 0;
  }
  def.compartments.forEach((_, i) => s.sys.life!.atmos.fill(s.st, i, 70));
  tick(s, 0.5);
  return s;
}

/** How much a violent decompression of a compartment could break (sum of its machines' sensitivity). */
const fragility = (def: ShipDef, comp: string) => def.parts.filter((p) => p.zone === comp).reduce((a, p) => a + p.decomp, 0);

function ctl(sim: ShipSim, id: string) {
  const c = sim.def.controls.find((x) => x.id === id);
  if (!c) throw new Error(`no existe el mando ${id}`);
  return c;
}

/** Which control a ray into the face of `id` hits first. */
function aimed(sim: ShipSim, id: string) {
  const c = ctl(sim, id);
  const shut = shutCovers(sim.def.controls, sim.sw);
  const origin = madd(c.c, c.n, 0.35);
  const dir = scale(c.n, -1);
  let best: string | null = null;
  let bestT = 2;
  for (const x of sim.def.controls) {
    const hit = controlHit(x, sim.sw, shut);
    if (!hit) continue;
    const t = rayBox(hit.frame, hit.half, origin, dir, 1);
    if (t >= 0 && t < bestT) {
      bestT = t;
      best = x.id;
    }
  }
  return best;
}

function tick(sim: ShipSim, seconds: number) {
  const n = Math.round(seconds * SYSTEMS_HZ);
  for (let i = 0; i < n; i++) sim.tick(1 / SYSTEMS_HZ);
}

/** Uneven ground (the flight tests), and empty sky far below. */
const BUMPY = testSurface(0.35);
const SKY = testSurface(0, -5000);

/** Fly for `seconds` at 60 Hz with the systems at their own rate, as the flight authority does. */
function fly(s: ShipSim, seconds: number, cmd: FlightCommand = FLIGHT_IDLE, ground: BodySurface = BUMPY) {
  flyUntil(s, seconds, () => false, ground, cmd);
}

/** Fly until `done` (checked every step); returns the seconds it took (or `seconds`). */
function flyUntil(s: ShipSim, seconds: number, done: () => boolean, ground: BodySurface = BUMPY, cmd: FlightCommand = FLIGHT_IDLE) {
  let acc = 0;
  const n = Math.round(seconds * 60);
  for (let i = 0; i < n; i++) {
    s.flight.step(1 / 60, cmd, { surface: ground });
    acc += 1 / 60;
    while (acc >= 1 / SYSTEMS_HZ) {
      acc -= 1 / SYSTEMS_HZ;
      s.tick(1 / SYSTEMS_HZ);
    }
    if (done()) return i / 60;
  }
  return seconds;
}

/** In the air, gear up, well clear of the ground. */
function airborne(s: ShipSim) {
  const key = s.def.gear?.key;
  if (key) {
    s.sw[key] = 0;
    const i = s.sys.moverIndex(key);
    if (i !== undefined) s.st[i] = 0;
  }
  s.landed = false;
  s.onPad = false;
  s.pose.p[1] += 60;
  s.flight.wake();
}

/** Operate a control by switch key (the first one that moves it), as the crew would. */
function press(s: ShipSim, key: string) {
  const c = s.def.controls.find((x) => x.key === key && x.kind !== 'cover');
  if (!c) throw new Error(`ningún mando mueve ${key}`);
  const r = s.interact(c.index);
  if ('reason' in r) throw new Error(`${key}: ${r.reason}`);
}

const norm3 = (v: V3): V3 => {
  const l = Math.hypot(v[0], v[1], v[2]) || 1;
  return [v[0] / l, v[1] / l, v[2] / l];
};

function assert(cond: unknown, msg: string): asserts cond {
  if (!cond) throw new Error(msg);
}

function texts(sections: ManualSection[]) {
  const out: string[] = [];
  for (const s of sections) {
    for (const b of s.body) {
      if (typeof b === 'string') out.push(b);
      else if ('steps' in b) out.push(...b.steps);
      else if ('note' in b) out.push(b.note);
    }
  }
  return out;
}

const jobs: Array<{ name: string; run: () => string }> = [
  {
    name: 'definición (todas las naves): ayuda, tapas, manual, máquinas, botones de página',
    run: () => {
      const out: string[] = [];
      for (const def of SHIPS) {
        const ids = new Set(def.controls.map((c) => c.id));
        const keys = new Set(def.controls.map((c) => c.key));
        for (const c of def.controls) {
          assert(c.help && c.help.length > 8, `${def.id}: ${c.id} no explica qué hace`);
          if (c.guard) assert(keys.has(c.guard), `${def.id}: ${c.id} apunta a una tapa ${c.guard} que no existe`);
        }
        // every mount gets a pylon to the hull section at its height (stacked decks included)
        const mounts = new Set(def.parts.map((p) => p.mount).filter(Boolean));
        assert(nacellePylons(def.parts, def.modules).length === mounts.size, `${def.id}: no todas las góndolas tienen pilón`);
        const refs = texts(def.manual).join('\n').match(/\[\[([^\]]+)\]\]/g) ?? [];
        assert(refs.length > 0, `${def.id}: el manual no nombra ningún mando`);
        for (const raw of refs) assert(ids.has(raw.slice(2, -2)), `${def.id}: el manual cita ${raw.slice(2, -2)} y ese mando no existe`);
        for (const p of def.parts) assert(p.model && p.mass > 0, `${def.id}: ${p.id} sin modelo o sin masa`);
        assert(def.props.length > 0, `${def.id}: la nave no tiene props`);
        for (const screen of def.screens) {
          const bezels = def.controls.filter((c) => c.kind === 'bezel' && c.key === screen.id);
          assert(bezels.length === screen.pages.length, `${def.id}: ${screen.id}: ${bezels.length} botones para ${screen.pages.length} páginas`);
          for (const b of bezels) assert(b.value !== undefined && screen.pages[b.value] !== undefined, `${b.id} no apunta a una página`);
        }
        out.push(`${def.name}: ${def.controls.length} mandos, ${def.parts.length} máquinas, ${refs.length} citas`);
      }
      assert(nacellePylons(HAULER.parts, HAULER.modules).length === 2, 'se esperaban dos pilones de góndola en la Selene');
      return out.join(' · ');
    },
  },
  {
    name: 'manual (todas las naves): cada mando explica lo que hace; las fichas citadas existen',
    run: () => {
      const out: string[] = [];
      for (const def of SHIPS) {
        // generated "Name. POS / POS." lines are not an explanation; obvious ones (doors) may be short
        const lazy = def.controls.filter((c) => c.kind !== 'bezel' && c.kind !== 'cover' && (c.help.length < 60 || c.help.startsWith(`${c.name}.`)));
        assert(lazy.length === 0, `${def.id}: sin explicación de verdad: ${[...new Set(lazy.map((c) => c.key))].join(', ')}`);
        const keys = new Set(def.controls.map((c) => c.key));
        for (const s of def.manual) for (const b of s.body) if (typeof b === 'object' && 'controls' in b) for (const k of b.controls) assert(keys.has(k), `${def.id}/${s.id}: ficha de «${k}», que no es ningún mando`);
        const sys = sim(def).sys;
        const mute = sys.alerts.filter((a) => !a.help);
        assert(mute.length === 0, `${def.id}: alarmas sin explicación: ${mute.map((a) => a.id).join(', ')}`);
        out.push(`${def.name}: ${new Set(def.controls.map((c) => c.key)).size} mandos, ${sys.alerts.length} alarmas, ${def.manual.length} capítulos`);
      }
      return out.join(' · ');
    },
  },
  {
    name: 'pantalla del piloto: el botón de página cambia lo que se dibuja',
    run: () => {
      const sim = fresh();
      const screen = HAULER.screens.find((s) => s.id === 'mfd.pilot')!;
      assert(activePage(screen, sim.sw) === 'flight', `página inicial ${activePage(screen, sim.sw)}`);
      const page = 2;
      const r = sim.interact(ctl(sim, `ck.main/mfd.pilot=${page}`).index);
      assert(!('reason' in r), 'reason' in r ? r.reason : '');
      assert(activePage(screen, sim.sw) === screen.pages[page], `se quedó en ${activePage(screen, sim.sw)}`);
      return `${screen.pages[0]} → ${screen.pages[page]}`;
    },
  },
  {
    name: 'tapa de seguridad: cerrada bloquea el armado, abierta lo deja',
    run: () => {
      const sim = fresh();
      const arm = ctl(sim, 'ck.pl/eng.L.arm');
      const closed = sim.blocked(arm);
      assert(closed?.includes('Tapa'), closed ?? 'el armado no estaba bloqueado');
      const r = sim.interact(ctl(sim, 'ck.pl/guard.eng.L').index);
      assert(!('reason' in r) && sim.sw['guard.eng.L'] === 1, 'la tapa no se abrió');
      assert(sim.blocked(arm) === null, sim.blocked(arm) ?? '');
      return 'tapa abierta, armado libre';
    },
  },
  {
    name: 'tapa: cerrada cubre su mando, abierta lo deja, y no tapa al de al lado',
    run: () => {
      const sim = fresh();
      assert(aimed(sim, 'ck.pl/eng.L.arm') === 'ck.pl/guard.eng.L', `cerrada, el rayo dio en ${aimed(sim, 'ck.pl/eng.L.arm')}`);
      assert(aimed(sim, 'ck.pl/eng.L.start') === 'ck.pl/eng.L.start', `el arranque de al lado dio en ${aimed(sim, 'ck.pl/eng.L.start')}`);
      assert(aimed(sim, 'ck.pl/nacelle') === 'ck.pl/nacelle', `la palanca dio en ${aimed(sim, 'ck.pl/nacelle')}`);
      sim.interact(ctl(sim, 'ck.pl/guard.eng.L').index);
      assert(aimed(sim, 'ck.pl/eng.L.arm') === 'ck.pl/eng.L.arm', `abierta, el rayo dio en ${aimed(sim, 'ck.pl/eng.L.arm')}`);
      const lid = coverHit(ctl(sim, 'ck.pl/guard.eng.L'), true);
      const origin = madd(lid.frame.c, lid.frame.n, 0.2);
      const dir = scale(lid.frame.n, -1);
      const t = rayBox(lid.frame, lid.half, origin, dir, 1);
      assert(t >= 0, 'la tapa abierta no recibe el clic');
      const arm = controlHit(ctl(sim, 'ck.pl/eng.L.arm'), sim.sw, shutCovers(sim.def.controls, sim.sw));
      const through = arm ? rayBox(arm.frame, arm.half, origin, dir, 1) : -1;
      assert(through < 0 || through > t, 'la tapa abierta sigue por delante del mando');
      return 'placa fina: el de dentro y los vecinos se pueden pulsar';
    },
  },
  {
    name: 'una negativa tiene que aguantar un momento antes de contar',
    run: () => {
      const hold = new ReasonHold(0.3);
      assert(hold.live('start', 'Motor ya en marcha', 1) === null, 'avisó en el mismo instante');
      assert(hold.live('start', 'Motor ya en marcha', 1.2) === null, 'avisó antes de tiempo');
      assert(hold.live('start', 'Motor ya en marcha', 1.3) === 'Motor ya en marcha', 'no avisó cuando ya se había mantenido');
      assert(hold.live('start', 'Otra causa', 1.31) === null, 'una causa nueva no heredó el tiempo de la anterior');
      assert(hold.live('reset', 'El reactor no está en SCRAM', 5) === null, 'el rearme avisó al momento');
      hold.pin('reset');
      assert(hold.live('reset', 'El reactor no está en SCRAM', 5) === 'El reactor no está en SCRAM', 'un rechazo del servidor no encendió el botón');
      assert(hold.live('start', null, 2) === null, 'al desaparecer la causa siguió contando');
      assert(hold.live('start', 'Motor ya en marcha', 2) === null, 'al volver la causa no empezó de cero');
      return '0.3 s, y cada mando por su cuenta';
    },
  },
  {
    name: 'selector de potencia: se detiene en los extremos',
    run: () => {
      const sim = fresh();
      const knob = ctl(sim, 'cg.rct/rx.set');
      assert(knob.wrap === false, 'rx.set da la vuelta');
      assert(sim.sw['rx.set'] === 2, `salida inicial ${sim.sw['rx.set']}`);
      sim.interact(knob.index, -1);
      sim.interact(knob.index, -1);
      const stay = sim.interact(knob.index, -1);
      assert(sim.sw['rx.set'] === 0 && !('reason' in stay) && Object.keys(stay.changed).length === 0, `bajó de 0 a ${sim.sw['rx.set']}`);
      sim.sw['rx.set'] = knob.states.length - 1;
      sim.interact(knob.index, 1);
      assert(sim.sw['rx.set'] === knob.states.length - 1, `pasó del máximo a ${sim.sw['rx.set']}`);
      return `0 … ${knob.states.length - 1}, sin vuelta`;
    },
  },
  {
    name: 'puerta: el disyuntor cerrado acepta la orden aunque el bus esté a cero',
    run: () => {
      const sim = fresh();
      const door = ctl(sim, 'ck.main/door.cockpit');
      sim.sw['brk.doors'] = 0;
      const dead = sim.blocked(door);
      assert(dead?.includes('PUERTAS'), dead ?? 'no avisó del disyuntor');
      sim.sw['brk.doors'] = 1;
      sim.st[sim.sys.power.fIndex('doors')] = 0;
      const starved = sim.blocked(door);
      assert(starved === null || !starved.includes('Sin energía'), starved ?? '');
      return 'disyuntor abierto bloquea; bus a 0 % no';
    },
  },
  {
    name: 'nave abierta: la cabina sigue al vacío',
    run: () => {
      const sim = fresh();
      tick(sim, 10);
      const p = sim.def.compartments.map((c) => `${c.id} ${sim.sys.pressure(sim.st, c.id).toFixed(2)} kPa`);
      for (const c of sim.def.compartments) assert(sim.sys.pressure(sim.st, c.id) < 1, `${c.id} se presurizó con la rampa abierta`);
      return p.join(' · ');
    },
  },
  {
    name: 'cerrar rampa y puertas: la presión sube',
    run: () => {
      const sim = fresh();
      sim.sw.ramp = 0;
      sim.sw['door.cockpit'] = 0;
      sim.sw['door.cargo'] = 0;
      let closed = false;
      for (let s = 0; s < 12 && !closed; s++) {
        tick(sim, 1);
        closed = sim.mover('ramp') < 0.05 && sim.mover('door.cockpit') < 0.05 && sim.mover('door.cargo') < 0.05;
      }
      assert(closed, `no llegaron a cerrar (rampa ${sim.mover('ramp').toFixed(2)})`);
      const before = sim.sys.pressure(sim.st, 'cargo');
      tick(sim, 15);
      const after = sim.def.compartments.map((c) => ({ id: c.id, p: sim.sys.pressure(sim.st, c.id) }));
      const cargo = after.find((c) => c.id === 'cargo')!;
      assert(cargo.p > before + 5, `bodega ${before.toFixed(1)} → ${cargo.p.toFixed(1)} kPa`);
      for (const c of after) assert(c.p > 1, `${c.id} no subió (${c.p.toFixed(2)} kPa)`);
      return after.map((c) => `${c.id} ${c.p.toFixed(1)} kPa`).join(' · ');
    },
  },
  {
    name: 'reactor: sin bomba de refrigerante no rearranca',
    run: () => {
      const sim = fresh();
      const reactor = ctl(sim, 'cg.rct/reactor');
      assert(sim.sw.reactor === 1, 'el reactor no arrancaba encendido');
      const off = sim.interact(reactor.index);
      assert(!('reason' in off) && sim.sw.reactor === 0, 'reason' in off ? off.reason : 'no se paró');
      const pump = sim.interact(ctl(sim, 'cg.rct/coolpump').index);
      assert(!('reason' in pump) && sim.sw.coolpump === 0, 'la bomba no se apagó');
      // still winding down: raising the lever again just keeps it running
      assert(sim.blocked(reactor) === null, `bajando potencia se negó: ${sim.blocked(reactor)}`);
      tick(sim, 12);
      assert(sim.get('rx.state') === RX.off, `el reactor no llegó a pararse (estado ${sim.get('rx.state')})`);
      const why = sim.blocked(reactor);
      assert(why?.toLowerCase().includes('bomba'), why ?? 'el rearranque no se negó');
      return why!;
    },
  },
  {
    name: 'anunciador (todas las naves): cada grupo de alarmas tiene su lámpara',
    run: () =>
      SHIPS.map((def) => {
        const s = sim(def);
        const lamps = new Set(def.annunciator?.lamps ?? []);
        const missing = s.sys.lampGroups().filter((l) => !lamps.has(l));
        assert(missing.length === 0, `${def.id}: alarmas sin lámpara en el tablero: ${missing.join(', ')}`);
        return `${def.name} ${s.sys.alerts.length} alarmas en ${lamps.size} lámparas`;
      }).join(' · '),
  },
  {
    name: 'datos (todas las naves): nada cita algo que no exista',
    run: () => {
      for (const def of SHIPS) {
        const problems = checkShip(def);
        assert(problems.length === 0, `${def.id}: ${problems.join(' · ')}`);
      }
      const typo = { ...HAULER, loads: [...HAULER.loads, { key: 'x', circuit: 'luces', kw: 1 }] };
      assert(checkShip(typo).some((p) => p.includes('luces')), 'un circuito mal escrito pasó sin aviso');
      return SHIPS.map((d) => `${d.name}: ${d.loads.length} cargas, ${d.movers.length} mecanismos, ${d.parts.length} máquinas`).join(' · ');
    },
  },
  {
    name: 'selectores dan la vuelta, escalas no',
    run: () => {
      const sim = fresh();
      const xfer = ctl(sim, 'ck.cp/xfer');
      sim.sw.xfer = xfer.states.length - 1;
      sim.interact(xfer.index);
      assert(sim.sw.xfer === 0, `la transferencia se quedó en ${sim.sw.xfer}`);
      assert(ctl(sim, 'ck.cp/radar.range').wrap === false, 'el alcance del radar da la vuelta');
      return 'TRANSF. 4 → 0; alcance y potencia se paran';
    },
  },
  {
    name: 'rampa: no baja con la bodega presurizada (enclavamiento de la abertura)',
    run: () => {
      const sim = fresh();
      sim.sw.ramp = 0;
      sim.sw['door.cockpit'] = 0;
      sim.sw['door.cargo'] = 0;
      tick(sim, 40);
      const p = sim.sys.pressure(sim.st, 'cargo');
      assert(p > 5, `la bodega solo llegó a ${p.toFixed(1)} kPa`);
      const why = sim.blocked(ctl(sim, 'ext.ramp/ramp'));
      assert(why?.includes('presurizada'), why ?? 'la rampa se dejaba abrir');
      const door = sim.blocked(ctl(sim, 'bk2.b/door.cargo'));
      assert(door === null || !door.includes('diferencia'), `puerta entre dos compartimentos con aire: ${door}`);
      return why!;
    },
  },
  {
    name: 'APU: un fallo se queda enclavado (no reintenta sola)',
    run: () => {
      const sim = fresh();
      sim.st[sim.sys.hpIndex('apu')] = 5; // badly damaged: fails while spooling
      sim.sw.apu = 1;
      let says = 0;
      let fails = 0;
      const rand = () => 0; // every random failure happens
      for (let i = 0; i < 20 * 20; i++) {
        const r = sim.tick(1 / SYSTEMS_HZ, { rand });
        says += r.events.filter((e) => e.type === 'say').length;
        if (sim.get('apu.state') === ENG.fail) fails++;
      }
      assert(sim.get('apu.state') === ENG.fail, `estado ${sim.get('apu.state')}`);
      assert(says === 1, `${says} mensajes de fallo en 20 s (debería avisar una vez)`);
      sim.sw.apu = 0;
      sim.tick(1 / SYSTEMS_HZ);
      assert(sim.get('apu.state') === ENG.off, 'apagarla no rearma el fallo');
      return `1 aviso, ${fails} ticks en FALLO, se rearma al apagarla`;
    },
  },
  {
    name: 'motor destruido: no se deja arrancar',
    run: () => {
      const sim = fresh();
      sim.sw['guard.eng.L'] = 1;
      sim.sw['eng.L.arm'] = 1;
      sim.st[sim.sys.hpIndex('eng.L')] = 0;
      const why = sim.blocked(ctl(sim, 'ck.pl/eng.L.start'));
      assert(why?.includes('destruido'), why ?? 'se dejaba arrancar');
      return why!;
    },
  },
  {
    name: 'umbilical del asiento: sin O₂ en la botella no recarga el traje',
    run: () => {
      const sim = fresh();
      const seat = sim.def.seats[0];
      const at = sim.toWorld(seat.root);
      const suit = () => crewStep([sim], [{ p: at, seated: true, o2: 0.5 }], 1, (s, ctx) => s.tick(1, ctx)).o2[0];
      suit(); // publish the umbilical state
      const fed = suit();
      assert(fed > 0.5, `con O₂ el traje no subió (${fed.toFixed(4)})`);
      sim.st[sim.vars.idx('gas.O2.kg')] = 0;
      suit();
      const dry = suit();
      assert(dry < 0.5, `sin O₂ el traje subió igual (${dry.toFixed(4)})`);
      return `con botella ${fed.toFixed(4)}, vacía ${dry.toFixed(4)}`;
    },
  },
  {
    name: 'nave mínima hecha desde cero: solo batería, luces y una cabina',
    run: () => {
      const B = new ShipBuilder();
      const mod = { zone: 'cab', z0: -2, z1: 2, profile: [[-1, 0], [-1, 2], [1, 2], [1, 0]] as Array<[number, number]>, cols: 2 };
      B.strip(mod, 'P', ['L', 'T', 'R'], [1, 1, 1], () => 'hull', 0.1);
      B.floor('cab', 'P', -1, 1, -2, 2, 1, 2, 0.08);
      const def = finishShip({
        id: 'pod',
        name: 'Cápsula',
        registry: 'POD-1',
        floorHeight: 0.5,
        panels: B.panels,
        modules: [mod],
        bounds: { min: [-1.2, -0.2, -2.2], max: [1.2, 2.2, 2.2] },
        zones: [{ id: 'cab', label: 'CABINA', min: [-1, 0, -2], max: [1, 2, 2], lights: [[0, 1.9, 0]], lightKey: 'light' }],
        compartments: [{ id: 'cab', label: 'CABINA', volume: 16 }],
        subsystems: [{ id: 'main', label: 'PRINCIPAL', breaker: 'brk.main', routes: [], color: 0xffffff, rating: 5, base: 0.1, priority: 'pri.main' }],
        loads: [{ key: 'light', circuit: 'main', kw: 0.3 }],
        parts: buildParts([{ id: 'bat', type: 'battery', name: 'Batería', c: [0, 0.3, 1.5], half: [0.2, 0.3, 0.2], zone: 'cab', maxHp: 50, sw: { on: 'bat' }, p: { kwh: 2 } }]),
        defaults: { bat: 1, light: 1, 'cab.p0': 70 },
      });
      const sim = new ShipSim(9, def, placeShip(def, at(ground), 0, 0, 0), alt(ground));
      const ids = sim.sys.modules.map((m) => m.id);
      assert(!ids.some((i) => i.startsWith('reactor') || i.startsWith('engine')), `módulos de más: ${ids.join(', ')}`);
      tick(sim, 60);
      const soc = sim.get('bat.soc');
      assert(sim.powered('main') && soc < 0.92, `circuito ${sim.powered('main')}, batería ${soc}`);
      assert(sim.sys.pressure(sim.st, 'cab') > 60, `cabina a ${sim.sys.pressure(sim.st, 'cab').toFixed(1)} kPa`);
      let refused = false;
      try {
        finishShip({ ...def, parts: buildParts([{ id: 'x', type: 'warpcore', name: 'X', c: [0, 0, 0], half: [0.1, 0.1, 0.1], zone: 'cab', maxHp: 1 }]) });
        new ShipSim(10, { ...def, parts: buildParts([{ id: 'x', type: 'warpcore', name: 'X', c: [0, 0, 0], half: [0.1, 0.1, 0.1], zone: 'cab', maxHp: 1 }]) }, placeShip(def, at(ground), 0, 0, 0), alt(ground));
      } catch {
        refused = true;
      }
      assert(refused, 'una máquina de un tipo que ningún sistema maneja pasó sin error');
      return `módulos: ${ids.join(', ')} · batería ${(soc * 100).toFixed(1)} % tras 60 s`;
    },
  },
  {
    name: 'mecánica nueva: un módulo añadido a la lista participa en las fases',
    run: () => {
      // a toy system: a "heater coil" part that draws power while its switch is on and warms a var
      const heater: SystemFactory = {
        id: 'coil',
        parts: ['coil'],
        make: (sys) =>
          sys.def.parts
            .filter((p) => p.type === 'coil')
            .map((p) => {
              const iT = sys.vars.define(`${p.id}.t`, 0.1, 0);
              return {
                id: `coil:${p.id}`,
                loads: (t) => {
                  if (t.sw[p.id] === 1) t.load(p.circuit, 2);
                },
                step: (t) => {
                  if (sys.supply(t.st, p.circuit) >= 0.5 && t.sw[p.id] === 1) t.st[iT] += t.dt;
                },
                alerts: () => [{ id: `hot.${p.id}`, label: 'BOBINA CALIENTE', level: 1, lamp: 'BOBINA', on: (st) => st[iT] > 1 }],
              };
            }),
      };
      SYSTEM_FACTORIES.push(heater);
      try {
        const parts = buildParts([...HAULER.parts.map(({ index: _i, ...p }) => p), { id: 'coil', type: 'coil', name: 'Bobina', c: [0, 1, 0], half: [0.1, 0.1, 0.1], zone: 'cargo', maxHp: 10, circuit: 'grav' }]);
        const def = { ...HAULER, parts, defaults: { ...HAULER.defaults, coil: 1 } };
        const sim = new ShipSim(11, def, placeShip(def, at(ground), 0, 0, 0), alt(ground));
        tick(sim, 2);
        assert(sim.get('coil.t') > 1.5, `la bobina no calentó (${sim.get('coil.t')})`);
        assert(sim.get('ckt.grav.kw') > 2, `no se vio su consumo (${sim.get('ckt.grav.kw')} kW)`);
        assert(sim.sys.active(sim.st).some((a) => a.id === 'hot.coil'), 'su alarma no saltó');
        return `bobina ${sim.get('coil.t').toFixed(1)} s · circuito ${sim.get('ckt.grav.kw').toFixed(1)} kW · alarma activa`;
      } finally {
        SYSTEM_FACTORIES.pop();
      }
    },
  },
  {
    name: 'reactor: con la palanca en PARADO por defecto arranca frío',
    run: () => {
      const def = { ...HAULER, defaults: { ...HAULER.defaults, reactor: 0 } };
      const sim = new ShipSim(12, def, placeShip(def, at(ground), 0, 0, 0), alt(ground));
      assert(sim.get('rx.state') === RX.off, `estado ${sim.get('rx.state')}`);
      tick(sim, 1);
      assert(sim.get('pwr.gen') < 0.05, `genera ${sim.get('pwr.gen')} kW apagado`);
      return `parado, ${sim.get('rx.temp').toFixed(0)} °C, ${Math.round(sim.get('bat.soc') * 100)} % batería`;
    },
  },
  {
    name: 'soldar una máquina sube su integridad',
    run: () => {
      const sim = fresh();
      const part = sim.def.parts.find((p) => p.id === 'reactor')!;
      sim.st[sim.sys.hpIndex(part.id)] = 10;
      const hp = sim.repairPart(part.index, 20);
      assert(hp !== null && hp > 10, `integridad ${hp}`);
      return `reactor ${hp!.toFixed(1)} / ${part.maxHp}`;
    },
  },
  // --- decompression ---------------------------------------------------------------------------
  {
    name: 'descompresión explosiva (todas las naves): la brecha vacía el compartimento en segundos, lo anuncia y daña lo frágil junto a ella, no lo robusto',
    run: () => {
      const out: string[] = [];
      for (const def of SHIPS) {
        const s = sealed(def);
        // the room with the most fragile machinery, blown open right beside its most fragile machine
        const comp = def.compartments.map((c) => c.id).sort((a, b) => fragility(def, b) - fragility(def, a))[0];
        const soft = def.parts.filter((p) => p.zone === comp).sort((a, b) => b.decomp - a.decomp)[0];
        const pn = def.panels.filter((p) => p.zone === comp && p.other === undefined && p.kind !== 'floor').sort((a, b) => s.sys.partDistance(soft, a.c) - s.sys.partDistance(soft, b.c))[0];
        const hp0 = new Map(def.parts.map((p) => [p.id, s.partHp(p)]));
        s.hp[pn.index] = 0;
        const said: string[] = [];
        let shock = 0;
        for (let k = 0; k < 3 * SYSTEMS_HZ; k++) {
          for (const e of s.tick(1 / SYSTEMS_HZ).events) if (e.type === 'say') said.push(e.text);
          shock = Math.max(shock, s.get(`${comp}.shock`));
        }
        const p = s.sys.pressure(s.st, comp);
        assert(p < 5, `${def.id}: ${comp} sigue a ${p.toFixed(1)} kPa tras 3 s con ${pn.id} reventado`);
        assert(shock > 0.5, `${def.id}: el choque publicado no pasó de ${shock.toFixed(2)}`);
        assert(said.some((t) => t.includes('DESCOMPRESIÓN EXPLOSIVA')), `${def.id}: no avisó (${said.join(' | ')})`);
        const lost = (id: string) => {
          const part = def.parts.find((x) => x.id === id)!;
          return (hp0.get(id)! - s.partHp(part)) / part.maxHp;
        };
        assert(lost(soft.id) > 0.25, `${def.id}: ${soft.id} junto a la brecha solo perdió ${(lost(soft.id) * 100).toFixed(0)} %`);
        for (const r of def.parts.filter((x) => x.zone === comp && x.decomp <= 0.05)) assert(lost(r.id) < 0.12, `${def.id}: ${r.id} (robusto) perdió ${(lost(r.id) * 100).toFixed(0)} %`);
        out.push(`${def.name}: ${comp} a ${p.toFixed(1)} kPa, ${soft.id} −${(lost(soft.id) * 100).toFixed(0)} %`);
        // the same air let out through a vent valve: slow, no shock, nothing broken
        const v = sealed(def);
        const vent = def.openings.find((o) => o.kind === 'vent' && o.a === comp)!;
        v.sw[vent.key] = 1;
        let vs = 0;
        for (let k = 0; k < 10 * SYSTEMS_HZ; k++) {
          v.tick(1 / SYSTEMS_HZ);
          vs = Math.max(vs, v.get(`${comp}.shock`));
        }
        assert(vs === 0, `${def.id}: el venteo cuenta como descompresión explosiva (${vs.toFixed(2)})`);
        assert(def.parts.every((x) => v.partHp(x) >= x.maxHp - 1e-6), `${def.id}: el venteo rompió algo`);
      }
      return out.join(' · ');
    },
  },
  {
    name: 'panel dañado bajo presión: avisa, se raja y revienta; soldado a tiempo aguanta; sin aire no cede',
    run: () => {
      const glass = (s: ShipSim) => s.def.panels.find((p) => p.kind === 'glass' && s.sys.compIndex(p.zone) >= 0)!;
      // cracked window in a pressurised room
      const s = sealed(HAULER);
      const g = glass(s);
      s.hp[g.index] = 33;
      let alarm = false;
      let reported = false;
      let t = 0;
      for (; t < 15 && !s.hole(g.index); t += 1 / SYSTEMS_HZ) {
        const r = s.tick(1 / SYSTEMS_HZ);
        reported ||= r.hp.some(([i]) => i === g.index);
        alarm ||= s.sys.active(s.st).some((a) => a.id === 'strain');
      }
      assert(s.hole(g.index), `${g.id} a 33/${g.maxHp} con ${s.sys.pressure(s.st, g.zone).toFixed(0)} kPa no reventó en 15 s (${s.hp[g.index].toFixed(1)})`);
      assert(t > 2, `reventó sin dar tiempo a reaccionar (${t.toFixed(2)} s)`);
      assert(alarm && reported, `alarma ${alarm}, cambios de integridad difundidos ${reported}`);
      // the same window welded above its limit before it goes: holds
      const w = sealed(HAULER);
      w.hp[g.index] = 33;
      w.tick(1 / SYSTEMS_HZ);
      w.repair(g.index, 15);
      for (let k = 0; k < 10 * SYSTEMS_HZ; k++) w.tick(1 / SYSTEMS_HZ);
      assert(!w.hole(g.index) && w.hp[g.index] > 45, `soldada a tiempo cedió igual (${w.hp[g.index].toFixed(1)})`);
      // no air behind it: nothing to tear it
      const e = fresh();
      e.hp[g.index] = 33;
      tick(e, 5);
      assert(e.hp[g.index] === 33, `sin presión se rajó (${e.hp[g.index]})`);
      return `${g.id} reventó a los ${t.toFixed(1)} s · soldada aguanta a ${w.hp[g.index].toFixed(0)}`;
    },
  },
  {
    name: 'el aire de la brecha: tira hacia ella, más cerca más fuerte; agachado menos; fuera, el chorro empuja hacia fuera',
    run: () => {
      const s = sealed(HAULER);
      const pn = s.def.panels.find((p) => p.zone === 'cargo' && p.kind === 'hull' && p.n[0] > 0.9)!;
      const ci = s.sys.compIndex('cargo');
      assert(s.vents().length === 0, `sellada ya hay chorros: ${s.vents().length}`);
      s.hp[pn.index] = 0;
      const vents = s.vents();
      const v = vents.find((x) => x.panel === pn.index);
      assert(v && v.down === -1 && v.dir[0] > 0.9, `la brecha no sopla hacia fuera (${JSON.stringify(v?.dir)})`);
      const at = (d: number): V3 => [pn.c[0] - d, pn.c[1], pn.c[2]];
      const pull = (d: number, cda = DRAG.standing) => airAccel(airPush(s.def, vents, at(d), ci), cda);
      const near = pull(0.9);
      const far = pull(4);
      assert(near[0] > 3, `a 0,9 m tira ${near[0].toFixed(2)} m/s² hacia la brecha`);
      assert(far[0] > 0 && far[0] < near[0] && far[0] < 2.6, `a 4 m tira ${far[0].toFixed(2)} m/s² (cerca ${near[0].toFixed(2)}): o empuja al revés o arrastra a cualquiera`);
      assert(pull(0.9, DRAG.crouched)[0] < near[0], 'agachado tira lo mismo');
      assert(pull(4, boxDrag([0.35, 0.3, 0.35], 40))[0] > far[0], 'una caja pesa menos por superficie que un astronauta y debería volar antes');
      const plume = airAccel(airPush(s.def, vents, [pn.c[0] + 2, pn.c[1], pn.c[2]], -1), DRAG.standing);
      assert(plume[0] > 5, `fuera, a 2 m del agujero, el chorro empuja ${plume[0].toFixed(1)} m/s²`);
      return `a 0,9 m ${near[0].toFixed(1)} m/s², a 4 m ${far[0].toFixed(2)} m/s², chorro a 2 m fuera ${plume[0].toFixed(1)} m/s²`;
    },
  },
  {
    name: 'la brecha empuja la nave (todas las naves): en vuelo se aparta del chorro; posada la sacude sin volcarla',
    run: () => {
      const out: string[] = [];
      for (const def of SHIPS) {
        const comp = [...def.compartments].sort((a, b) => b.volume - a.volume)[0].id;
        const pn = def.panels.find((p) => p.zone === comp && p.kind === 'hull' && p.n[0] > 0.9)!;
        const run = (breach: boolean, air: boolean) => {
          const s = sealed(def);
          if (air) airborne(s);
          s.flight.wake();
          fly(s, 1, FLIGHT_IDLE, air ? SKY : ground);
          const p0 = [...s.pose.p];
          if (breach) s.hp[pn.index] = 0;
          fly(s, 3, FLIGHT_IDLE, air ? SKY : ground);
          return { dx: s.pose.p[0] - p0[0], s };
        };
        const a = run(true, true).dx - run(false, true).dx;
        assert(a < -0.3, `${def.id}: en vuelo el chorro a +x la movió ${a.toFixed(2)} m en x`);
        const g = run(true, false).s;
        assert(g.landed && levelness(g.pose.q) > 0.99, `${def.id}: posada, la brecha la dejó ${g.landed ? '' : 'en el aire '}inclinada ${levelness(g.pose.q).toFixed(4)}`);
        out.push(`${def.name} ${a.toFixed(1)} m`);
      }
      return out.join(' · ');
    },
  },
  // --- catalog ---------------------------------------------------------------------------------
  {
    name: 'catálogo: cada componente lo mueve un sistema, tiene ficha y números propios',
    run: () => {
      const claimed = new Set(SYSTEM_FACTORIES.flatMap((f) => f.parts ?? []));
      const list = allComponents();
      for (const c of list) {
        assert(claimed.has(c.type), `${c.id}: ningún sistema maneja el tipo "${c.type}"`);
        assert(['XS', 'S', 'M', 'L'].includes(c.size), `${c.id}: tamaño ${c.size}`);
        assert(c.half.every((h) => h > 0) && c.maxHp > 0 && c.mass > 0, `${c.id}: caja, integridad o masa sin sentido`);
        assert(componentSheet(c).length >= 2, `${c.id}: ficha vacía`);
        assert(c.desc.length > 10, `${c.id}: sin descripción`);
      }
      for (const f of allFurniture()) assert(f.half.every((h) => h > 0), `mueble ${f.id}: caja vacía`);
      // the same machine one size up is never weaker
      const byType = new Map<string, typeof list>();
      for (const c of list) byType.set(c.type, [...(byType.get(c.type) ?? []), c]);
      const order = ['XS', 'S', 'M', 'L'];
      for (const [type, cs] of byType) {
        const main = type === 'reactor' ? 'kw' : type === 'battery' ? 'kwh' : type === 'engine' ? 'thrustN' : null;
        if (!main) continue;
        const sorted = [...cs].sort((a, b) => order.indexOf(a.size) - order.indexOf(b.size));
        for (let i = 1; i < sorted.length; i++) assert(sorted[i].p[main] > sorted[i - 1].p[main], `${sorted[i].id} no supera a ${sorted[i - 1].id} en ${main}`);
      }
      return `${list.length} componentes de ${byType.size} tipos, ${allFurniture().length} muebles`;
    },
  },
  {
    name: 'catálogo: las dos naves se construyen solo con componentes; uno propio hereda del estándar',
    run: () => {
      for (const def of SHIPS) for (const p of def.parts) assert(p.component && component(p.component), `${def.id}: ${p.id} no viene del catálogo`);
      assert(HAULER.parts.find((p) => p.id === 'reactor')!.component === 'reactor.fission.M', 'la Selene no lleva el reactor M');
      assert(PEREGRINA.parts.find((p) => p.id === 'reactor')!.component === 'reactor.fission.XS', 'la Peregrina no lleva el reactor XS');
      const mine = defineComponent({ base: 'reactor.fission.S', id: 'test.reactor.hot', name: 'Reactor de pruebas', p: { kw: 42 } });
      const spec = part(mine, { id: 'rx', c: [0, 0, 0], zone: null, p: { startS: 3 } });
      assert(spec.p!.kw === 42 && spec.p!.startS === 3 && spec.p!.heatKw === component('reactor.fission.S').p.heatKw, `mezcla de parámetros ${JSON.stringify(spec.p)}`);
      const stretched = part('tank.cyl.S', { id: 't', c: [0, 0, 0], zone: null, half: [0.22, 0.22, 3] });
      assert(Math.abs(stretched.mass! - component('tank.cyl.S').mass * 2) < 1e-6, `masa estirada ${stretched.mass}`);
      return `${SHIPS.map((d) => `${d.name} ${d.parts.length} máquinas`).join(', ')}, todas del catálogo · propio: ${spec.p!.kw} kW, calor ${spec.p!.heatKw.toFixed(1)} kW heredado`;
    },
  },
  {
    name: 'reactores: cada tamaño se estabiliza por debajo de la alarma con radiadores a su escala',
    run: () => {
      const out: string[] = [];
      for (const size of ['XS', 'S', 'M', 'L']) {
        const rx = component(`reactor.fission.${size}`);
        // fixed radiators sized like the Selene's stowed wings: 8 m² per 60 kW
        const area = (8 * rx.p.kw) / 60;
        const B = new ShipBuilder();
        const mod = { zone: 'e', z0: -2, z1: 2, profile: [[-2, 0], [-2, 3], [2, 3], [2, 0]] as Array<[number, number]>, cols: 1 };
        B.floor('e', 'E', -2, 2, -2, 2, 1, 1, 0.08);
        const def = finishShip({
          id: `rx.${size}`,
          name: size,
          registry: size,
          floorHeight: 1,
          panels: B.panels,
          modules: [mod],
          bounds: { min: [-3, -1, -3], max: [3, 4, 3] },
          zones: [{ id: 'e', label: 'E', min: [-2, 0, -2], max: [2, 3, 2], lights: [], lightKey: 'l' }],
          subsystems: [{ id: 'cool', label: 'REFRIG', breaker: 'brk.cool', priority: 'pri.cool', routes: [], color: 0, rating: 50, base: 0 }],
          loads: [{ key: 'coolpump', circuit: 'cool', kw: 1 }],
          parts: buildParts([
            part(rx, { id: 'reactor', c: [0, 1, 0], zone: 'e', circuit: 'cool' }),
            part('coolpump.S', { id: 'coolpump', c: [1.5, 0.3, 0], zone: 'e', circuit: 'cool' }),
            part('radiator.panel.S', { id: 'rad', c: [0, 3.2, 0], zone: null, p: { stowed: area, deployed: area } }),
            part('battery.S', { id: 'bat', c: [-1.5, 0.5, 0], zone: 'e', sw: { on: 'bat' } }),
          ]),
          defaults: { reactor: 1, 'reactor.set': 4, coolpump: 1, bat: 1 },
        });
        const s = sim(def);
        tick(s, 240);
        const temp = s.get('reactor.temp');
        assert(s.get('reactor.state') === RX.online, `${size}: estado ${s.get('reactor.state')}`);
        assert(temp < 600 && temp > 200, `${size}: núcleo a ${temp.toFixed(0)} °C al 100 %`);
        out.push(`${size} ${rx.p.kw} kW → ${temp.toFixed(0)} °C`);
      }
      return out.join(' · ');
    },
  },
  // --- Peregrina ---------------------------------------------------------------------------------
  {
    name: 'Peregrina: 5 min con tres a bordo, sin alarmas, energía positiva, víveres gastándose',
    run: () => {
      const s = sim(PEREGRINA);
      const water0 = s.get('stores.water');
      const food0 = s.get('stores.food');
      for (let i = 0; i < 300 * SYSTEMS_HZ; i++) s.tick(1 / SYSTEMS_HZ, { crew: [1, 2, 0] });
      const active = s.sys.active(s.st).map((a) => a.id);
      assert(active.length === 0, `alarmas: ${active.join(', ')}`);
      assert(s.get('pwr.gen') > s.get('pwr.load'), `genera ${s.get('pwr.gen').toFixed(1)} kW para ${s.get('pwr.load').toFixed(1)}`);
      assert(s.get('bat.soc') > 0.9, `batería ${s.get('bat.soc')}`);
      const food = food0 - s.get('stores.food');
      assert(food > 0.05 && food < 0.2, `comida gastada ${food.toFixed(3)} kg`);
      const water = water0 - s.get('stores.water');
      assert(water > 0 && water < 0.1, `agua neta gastada ${water.toFixed(3)} kg (con reciclador)`);
      for (const c of ['cockpit', 'cabin']) assert(s.sys.breathable(s.st, c), `${c} no es respirable (${s.get(`${c}.po2`).toFixed(1)} kPa O₂)`);
      assert(s.get('cabin.po2') < 23, `el O₂ del habitáculo sube sin freno: ${s.get('cabin.po2').toFixed(1)} kPa`);
      return `red ${s.get('pwr.gen').toFixed(1)}/${s.get('pwr.load').toFixed(1)} kW (sol ${s.get('solar.kw').toFixed(1)}) · reactor ${s.get('rx.temp').toFixed(0)} °C · comida −${food.toFixed(2)} kg · agua −${(water * 1000).toFixed(0)} g`;
    },
  },
  {
    name: 'Peregrina: ciclo de esclusa entrar / salir sin perder el aire del habitáculo',
    run: () => {
      const s = sim(PEREGRINA);
      assert(s.get('lock.phase') === LOCK.out && s.mover('door.ext') > 0.99 && s.sys.pressure(s.st, 'lock') < 1, 'no empieza con la escotilla abierta y la esclusa vacía');
      const until = (x: ShipSim, what: () => boolean, max: number) => {
        let t = 0;
        while (!what() && t < max) {
          x.tick(1 / SYSTEMS_HZ);
          t += 1 / SYSTEMS_HZ;
        }
        return t;
      };
      s.interact(ctl(s, 'lk.ctl/lock.cycle=0').index);
      const tin = until(s, () => s.get('lock.phase') === LOCK.in, 60);
      assert(s.get('lock.phase') === LOCK.in, `entrada atascada en la fase ${s.get('lock.phase')}`);
      assert(s.mover('door.lock') > 0.98 && s.mover('door.ext') < 0.02 && s.sw['duct.lock'] === 1, 'al entrar: interior abierta, escotilla cerrada, conducto abierto');
      const kgBefore = s.get('gas.O2.kg') + s.get('gas.N2.kg');
      s.interact(ctl(s, 'cb.lock/lock.cycle=1').index);
      until(s, () => s.get('lock.phase') === LOCK.pump, 10);
      assert(s.sw['duct.lock'] === 0 && s.sw['door.lock'] === 0, 'vaciando con el conducto o la puerta interior abiertos');
      const tout = until(s, () => s.get('lock.phase') === LOCK.out, 90);
      assert(s.get('lock.phase') === LOCK.out, `salida atascada en la fase ${s.get('lock.phase')}`);
      assert(s.sys.pressure(s.st, 'cabin') > 65, `el habitáculo perdió aire: ${s.sys.pressure(s.st, 'cabin').toFixed(1)} kPa`);
      const recovered = s.get('gas.O2.kg') + s.get('gas.N2.kg') - kgBefore;
      assert(recovered > 4, `el compresor solo recuperó ${recovered.toFixed(1)} kg`);
      // by hand: the hatch refuses to open on a full airlock
      const s2 = sim(PEREGRINA);
      s2.sw['lock.cycle'] = 0;
      until(s2, () => s2.get('lock.phase') === LOCK.in, 60);
      const why = s2.blocked(ctl(s2, 'lk.ctl/door.ext'));
      assert(why?.includes('presurizada'), why ?? 'la escotilla se dejaba abrir con la esclusa llena');
      return `entrar ${tin.toFixed(0)} s · salir ${tout.toFixed(0)} s · ${recovered.toFixed(1)} kg de aire recuperados · a mano: «${why}»`;
    },
  },
  {
    name: 'Peregrina: el reciclador multiplica la autonomía de agua',
    run: () => {
      const run = (recycler: number) => {
        const s = sim(PEREGRINA);
        s.sw.recycler = recycler;
        const w0 = s.get('stores.water');
        for (let i = 0; i < 600 * SYSTEMS_HZ; i++) s.tick(1 / SYSTEMS_HZ, { crew: [1, 2, 0] });
        return { used: w0 - s.get('stores.water'), alert: s.sys.active(s.st).some((a) => a.id === 'recycler') };
      };
      const on = run(1);
      const off = run(0);
      assert(off.used > on.used * 6, `con reciclador ${on.used.toFixed(3)} kg, sin él ${off.used.toFixed(3)} kg`);
      assert(off.alert && !on.alert, 'la alarma del reciclador no sigue al interruptor');
      return `10 min, 3 a bordo: ${on.used.toFixed(2)} kg con reciclador, ${off.used.toFixed(2)} kg sin él (alarma AGUA)`;
    },
  },
  {
    name: 'solar: plegadas dan poco, desplegadas su potencia, de noche nada',
    run: () => {
      const s = sim(PEREGRINA);
      tick(s, 1);
      const open = s.get('solar.kw');
      s.sw.solar = 0;
      tick(s, 8);
      const folded = s.get('solar.kw');
      s.sun = 0;
      tick(s, 1);
      const night = s.get('solar.kw');
      const kw = component('solar.wing.S').p.kw;
      assert(Math.abs(open - kw) < 0.05 && folded < kw * 0.2 && folded > 0 && night === 0, `desplegadas ${open}, plegadas ${folded}, noche ${night}`);
      return `${open.toFixed(2)} → ${folded.toFixed(2)} → ${night} kW`;
    },
  },
  {
    name: 'acelerador (en vuelo desacoplado): el motor sigue al acelerador, gasta en proporción y empuja lo que dice su ficha',
    run: () => {
      const s = sim(PEREGRINA);
      for (const id of ['ck.l/guard.eng', 'ck.l/eng.arm', 'ck.l/eng.start']) {
        const r = s.interact(ctl(s, id).index);
        assert(!('reason' in r), `${id}: ${'reason' in r ? r.reason : ''}`);
        tick(s, 0.2);
      }
      tick(s, 4);
      assert(s.get('eng.state') === ENG.run, `motor en estado ${s.get('eng.state')}`);
      // in the air, decoupled: the lever is the engine's thrust; the pads carry the weight
      airborne(s);
      s.sw['fa.hold'] = 0;
      fly(s, 2, FLIGHT_IDLE, SKY);
      const idle = s.get('fuel.kg');
      fly(s, 10, FLIGHT_IDLE, SKY);
      const idleUse = idle - s.get('fuel.kg');
      const knob = ctl(s, 'ck.main/helm.throttle');
      for (let i = 0; i < 10; i++) s.interact(knob.index, 1);
      fly(s, 3, FLIGHT_IDLE, SKY);
      assert(s.get('eng.thr') > 0.95, `empuje ${s.get('eng.thr')}`);
      const full = s.get('fuel.kg');
      const v0 = s.dirToLocal(s.pose.v)[2];
      fly(s, 10, FLIGHT_IDLE, SKY);
      const fullUse = full - s.get('fuel.kg') - idleUse;
      const eng = s.def.parts.find((p) => p.id === 'eng')!;
      const flow = eng.p.flowKg ?? 0;
      assert(Math.abs(fullUse - flow * 10) < flow * 1.5, `el motor a tope gasta ${fullUse.toFixed(2)} kg en 10 s (ficha ${flow} kg/s)`);
      // Newton: the nose gains what thrust / mass says (it pushes toward −z)
      const dv = -(s.dirToLocal(s.pose.v)[2] - v0);
      const expect = ((eng.p.thrustN ?? 0) / s.massNow().mass) * 10;
      assert(Math.abs(dv - expect) < expect * 0.1, `Δv ${dv.toFixed(1)} m/s, esperado ${expect.toFixed(1)}`);
      return `motor a tope ${fullUse.toFixed(1)} kg en 10 s (ficha ${flow * 10}) · Δv ${dv.toFixed(1)} m/s (esperado ${expect.toFixed(1)})`;
    },
  },
  {
    name: 'escotilla lateral: el recorte deja el hueco libre y el resto del muro en pie',
    run: () => {
      const hatch = PEREGRINA.doors.find((d) => d.key === 'door.ext')!;
      const inside = PEREGRINA.panels.filter((p) => {
        if (p.zone !== 'lock' || Math.abs(p.n[0]) < 0.7) return false;
        return p.poly.some(([u, v]) => {
          const x = p.c[0] + p.u[0] * u + p.v[0] * v;
          const y = p.c[1] + p.u[1] * u + p.v[1] * v;
          const z = p.c[2] + p.u[2] * u + p.v[2] * v;
          return x > 1 && y < hatch.h - 0.02 && Math.abs(z - hatch.c[2]) < hatch.w / 2 - 0.02;
        });
      });
      assert(inside.length === 0, `paneles tapando la escotilla: ${inside.map((p) => p.id).join(', ')}`);
      const pieces = PEREGRINA.panels.filter((p) => p.zone === 'lock' && /^LK-R[12]-1[ab]$/.test(p.id));
      assert(pieces.length === 4, `trozos alrededor de la escotilla: ${pieces.map((p) => p.id).join(', ')}`);
      return `${pieces.length} trozos de muro alrededor de un hueco de ${hatch.w} × ${hatch.h} m`;
    },
  },
  {
    name: 'dos cubiertas (Albatros): el suelo alto separa las salas de arriba y abajo, una placa rota las une en vez de ventear, y la escotilla de la escalera es una puerta estanca',
    run: () => {
      const def = ALBATROS;
      const up = def.decks[1];
      assert(def.decks.length === 2 && up.y > 2, `cubiertas: ${def.decks.map((d) => `${d.id} ${d.y}`).join(', ')}`);
      const plates = def.panels.filter((p) => p.kind === 'floor' && Math.abs(p.c[1] + p.t / 2 - up.y) < 0.02);
      assert(plates.length > 20, `solo ${plates.length} placas en la cubierta superior`);
      for (const p of plates) {
        const under = zoneAtPoint(def.zones, [p.c[0], up.y - 0.8, p.c[2]]);
        assert(p.other === (under?.id ?? undefined), `${p.id}: dice que debajo está ${p.other ?? 'el vacío'}, pero está ${under?.id ?? 'el vacío'}`);
      }
      // the stairwell: no plate over the hole, the shut hatch fills it flush, the open one clears it
      const h = def.hatches[0];
      const over = plates.filter((p) => Math.abs(p.c[0] - h.c[0]) < h.w / 2 - 0.05 && Math.abs(p.c[2] - h.c[2]) < h.l / 2 - 0.05);
      assert(over.length === 0, `placas sobre el hueco de la escalera: ${over.map((p) => p.id).join(', ')}`);
      const shut = hatchPlate(h, 0);
      const open = hatchPlate(h, 1);
      assert(Math.abs(shut.c[1] + shut.half[1] - h.c[1]) < 1e-6 && Math.abs(shut.c[0] - h.c[0]) < 1e-6, `escotilla cerrada fuera de su hueco: ${shut.c}`);
      assert(open.c[0] - open.half[0] >= h.c[0] + h.w / 2 - 0.03 && open.c[1] + open.half[1] < h.c[1] - h.t, `escotilla abierta tapando el hueco: ${open.c}`);
      // a blown plate between the mess and the hold: the two share their air, nothing goes to vacuum
      const s = sealed(def);
      const plate = plates.find((p) => p.zone === 'mess' && p.other === 'hold')!;
      s.hp[plate.index] = 0;
      tick(s, 6);
      const pm = s.sys.pressure(s.st, 'mess');
      const ph = s.sys.pressure(s.st, 'hold');
      assert(pm > 50 && ph > 50 && Math.abs(pm - ph) < 3, `placa ${plate.id} rota: comedor ${pm.toFixed(1)} kPa, bodega ${ph.toFixed(1)} kPa`);
      // the bridge overhang has vacuum under it: that plate does vent
      const bow = plates.find((p) => p.zone === 'bridge' && p.other === undefined)!;
      const v = sealed(def);
      v.hp[bow.index] = 0;
      tick(v, 6);
      const pb = v.sys.pressure(v.st, 'bridge');
      assert(pb < 5, `placa ${bow.id} (bajo ella, el vacío) rota: el puente sigue a ${pb.toFixed(1)} kPa`);
      // the hatch refuses to open across a pressure difference, and carries the air once open
      const x = sealed(def);
      x.sys.life!.atmos.fill(x.st, x.sys.compIndex('eng'), 20);
      tick(x, 0.1);
      const hatch = ctl(x, `ms.hatch/${h.key}`);
      const why = x.blocked(hatch);
      assert(why?.includes('diferencia'), `escotilla con 50 kPa de diferencia: ${why ?? 'se dejaba abrir'}`);
      x.sys.life!.atmos.fill(x.st, x.sys.compIndex('eng'), 70);
      x.sw[h.key] = 1;
      tick(x, 4);
      const moved = x.st[x.sys.moverIndex(h.key)!];
      assert(moved > 0.99, `la escotilla no se abrió (${moved.toFixed(2)})`);
      return `${plates.length} placas en ${up.label.toLowerCase()} · ${plate.id} rota: ${pm.toFixed(0)}/${ph.toFixed(0)} kPa · ${bow.id} rota: puente a ${pb.toFixed(1)} kPa · «${why}»`;
    },
  },
  // --- flight (shared/ship/flight) ------------------------------------------------------------------
  {
    name: 'vuelo (todas las naves): masa, centro de masas, empuje/peso de la sustentación y del motor, Δv',
    run: () => {
      const out: string[] = [];
      for (const def of SHIPS) {
        const m = massProperties(def);
        const perf = performance(def, m, FLIGHT.g);
        assert(m.mass > 3000 && m.mass < 60000, `${def.id}: masa ${m.mass.toFixed(0)} kg`);
        assert(Math.abs(m.com[0]) < 0.4, `${def.id}: centro de masas desplazado ${m.com[0].toFixed(2)} m a un lado`);
        assert(perf.liftTwr > 1.3, `${def.id}: la sustentación no levanta la nave con margen (empuje/peso ${perf.liftTwr.toFixed(2)})`);
        assert(m.inertia[0] > 0 && m.inertia[1] > 0 && m.inertia[2] > 0, `${def.id}: inercia ${m.inertia}`);
        assert(def.autopilot && def.controls.some((c) => c.key === 'ap.on'), `${def.id}: sin panel de piloto automático`);
        out.push(`${def.name} ${(m.mass / 1000).toFixed(1)} t, VTOL ${perf.liftTwr.toFixed(2)}, motor ${perf.twr.toFixed(2)}, Δv ${perf.dv.toFixed(0)} m/s`);
      }
      // the rigid body alone: a force through the centre of mass accelerates it straight, a torque
      // about +y turns the nose to port
      const m = massProperties(PEREGRINA);
      const pose: ShipPose = { p: [0, 0, 0], q: qYaw(0), v: [0, 0, 0], w: [0, 0, 0] };
      for (let i = 0; i < 60; i++) integrateBody(pose, m, [0, 0, -m.mass], [0, 0, 0], 1 / 60);
      assert(Math.abs(pose.v[2] + 1) < 1e-6 && Math.abs(pose.p[2] + 0.5) < 0.02 && Math.hypot(...pose.w) < 1e-12, `recta: v ${pose.v} p ${pose.p}`);
      const spin: ShipPose = { p: [0, 0, 0], q: qYaw(0), v: [0, 0, 0], w: [0, 0, 0] };
      const I = m.inertia[1];
      for (let i = 0; i < 60; i++) integrateBody(spin, m, [0, 0, 0], qRotate(spin.q, [0, I, 0]), 1 / 60);
      assert(spin.w[1] > 0.9 && poseYaw(spin.q) > 0.3, `giro: w ${spin.w.map((v) => v.toFixed(3))} guiñada ${poseYaw(spin.q).toFixed(2)}`);
      return out.join(' · ');
    },
  },
  {
    name: 'vuelo: quieta en la pista (todas las naves) — no se hunde, no deriva, se duerme y sigue en tierra',
    run: () => {
      const out: string[] = [];
      for (const def of SHIPS) {
        const s = sim(def, 0.7, BUMPY);
        const p0 = [...s.pose.p];
        fly(s, 10, FLIGHT_IDLE, BUMPY);
        const d = Math.hypot(s.pose.p[0] - p0[0], s.pose.p[1] - p0[1], s.pose.p[2] - p0[2]);
        assert(s.landed && s.onPad, `${def.id}: en tierra ${s.landed}, en la plataforma ${s.onPad}`);
        assert(d < 0.08, `${def.id}: se movió ${d.toFixed(3)} m`);
        assert(s.flight.sleeping, `${def.id}: no se duerme en la pista`);
        assert(levelness(s.pose.q) > 0.995, `${def.id}: inclinada ${levelness(s.pose.q).toFixed(4)}`);
        const fuel = s.massNow().groups.propelente;
        fly(s, 5, FLIGHT_IDLE, BUMPY);
        assert(Math.abs(s.massNow().groups.propelente - fuel) < 1e-6, `${def.id}: gasta propelente aparcada`);
        out.push(`${def.name} ${(d * 100).toFixed(1)} cm`);
      }
      return out.join(' · ');
    },
  },
  {
    name: 'vuelo: estacionario (todas las naves) — R sube, al soltar frena, se queda quieta y la sustentación carga el peso',
    run: () => {
      const out: string[] = [];
      for (const def of SHIPS) {
        const s = sim(def, 0, BUMPY);
        fly(s, 1, FLIGHT_IDLE, BUMPY);
        fly(s, 4, { ...FLIGHT_IDLE, heave: 1 }, BUMPY);
        assert(!s.landed && s.flight.agl > 5, `${def.id}: no despega (AGL ${s.flight.agl.toFixed(2)} m)`);
        fly(s, 10, FLIGHT_IDLE, BUMPY);
        const y = s.pose.p[1];
        fly(s, 5, FLIGHT_IDLE, BUMPY);
        const t = s.flight.telemetry;
        const drift = Math.hypot(s.pose.v[0], s.pose.v[1], s.pose.v[2]);
        assert(drift < 0.1 && Math.abs(s.pose.p[1] - y) < 0.3, `${def.id}: no se queda quieta (v ${drift.toFixed(3)} m/s, Δy ${(s.pose.p[1] - y).toFixed(2)} m)`);
        assert(levelness(s.pose.q) > 0.999, `${def.id}: inclinada en estacionario (${levelness(s.pose.q).toFixed(4)})`);
        // thrifty: the pads carry the weight, the RCS nozzles don't fight each other
        assert(t.thrust < t.weight * 1.12, `${def.id}: empuja ${(t.thrust / 1000).toFixed(1)} kN para ${(t.weight / 1000).toFixed(1)} kN de peso`);
        out.push(`${def.name} ${t.agl.toFixed(1)} m, ${(t.thrust / t.weight).toFixed(3)} × peso`);
      }
      return out.join(' · ');
    },
  },
  {
    name: 'órbita: a 20 km con velocidad circular la nave sigue en órbita — el ordenador pasa a régimen orbital y no la frena ni la sostiene',
    run: () => {
      const s = sim(HAULER);
      airborne(s);
      tick(s, 1);
      const body = MOON_BODY;
      const h0 = 20000;
      const r0 = body.radius + h0;
      const vc = Math.sqrt(body.mu / r0);
      // over the base, flying east at the circular speed, level, acoupled flight left on
      s.pose.p[0] = 0;
      s.pose.p[1] = h0;
      s.pose.p[2] = 0;
      s.pose.v[0] = vc;
      s.pose.v[1] = 0;
      s.pose.v[2] = 0;
      s.pose.w.fill(0);
      s.sw['fa.hold'] = 1;
      let lo = Infinity;
      let hi = -Infinity;
      const steps = 120 * 60;
      for (let i = 0; i < steps; i++) {
        if (i % 3 === 0) s.tick(1 / SYSTEMS_HZ);
        s.flight.step(1 / 60, FLIGHT_IDLE, { surface: SKY });
        const h = altitudeOf(body, s.pose.p);
        lo = Math.min(lo, h);
        hi = Math.max(hi, h);
      }
      const o = orbitOf(body, s.pose.p, s.pose.v);
      assert(s.flight.orbital, 'no entró en régimen orbital');
      assert(lo > h0 - 1500 && hi < h0 + 1500, `la altura se fue a ${(lo / 1000).toFixed(1)}–${(hi / 1000).toFixed(1)} km (debería quedarse cerca de 20 km)`);
      assert(Math.abs(o.speed - vc) < 40, `la velocidad pasó de ${vc.toFixed(0)} a ${o.speed.toFixed(0)} m/s: algo la frena o la empuja`);
      assert(o.orbiting, `periápside ${(o.periapsis / 1000).toFixed(1)} km: no está en órbita`);
      const travelled = Math.hypot(s.pose.p[0], s.pose.p[2]);
      return `2 min en órbita: ${(lo / 1000).toFixed(2)}–${(hi / 1000).toFixed(2)} km de altura · ${o.speed.toFixed(0)} m/s (circular ${vc.toFixed(0)}) · AP ${(o.apoapsis / 1000).toFixed(1)} / PE ${(o.periapsis / 1000).toFixed(1)} km · ${(travelled / 1000).toFixed(0)} km recorridos`;
    },
  },
  {
    name: 'salida al espacio (Selene): despega, sube desacoplada y con el motor a tope llega a órbita estable, sin sostenerse con los propulsores al final',
    run: () => {
      const s = sim(HAULER);
      // engines armed and running (their switches, as the crew would leave them)
      const engines = s.sys.modules.filter((m): m is Engine => m.id.startsWith('engine:'));
      assert(engines.length > 0, 'la Selene no tiene motores');
      for (const e of engines) {
        s.sw[e.keys.arm] = 1;
        s.sw[e.keys.start] = 1;
      }
      tick(s, 6);
      for (const e of engines) assert(s.st[s.vars.idx(`${e.tag}.state`)] === ENG.run, `${e.part.name} no arrancó`);
      airborne(s);
      s.sw['fa.hold'] = 0;
      const helm = s.def.helm!;
      const knob = s.def.controls.find((c) => c.key === helm.throttle)!;
      const body = MOON_BODY;
      const fuel0 = s.get('fuel.kg');
      // climb a few kilometres first (clear of any ridge), then level off and push
      let t = 0;
      let lowest = Infinity;
      const step = (cmd: FlightCommand) => {
        if (Math.round(t * 60) % 3 === 0) s.tick(1 / SYSTEMS_HZ);
        s.flight.step(1 / 60, cmd, { surface: SKY });
        t += 1 / 60;
        lowest = Math.min(lowest, altitudeOf(body, s.pose.p));
      };
      // a pilot's ascent from 10 km (the climb itself is the stick's business): let coupled flight
      // settle the height, level the wings with the autopilot, decouple and open the throttle
      s.pose.p[1] += 10000;
      s.sw['fa.hold'] = 1;
      while (t < 400 && Math.abs(s.flight.telemetry.vs) > 0.5) step(FLIGHT_IDLE);
      s.sw['ap.on'] = 1;
      s.sw['ap.lvl'] = 1;
      s.sw['fa.hold'] = 0;
      s.sw[helm.throttle] = knob.states.length - 1;
      const burn0 = t;
      let o = orbitOf(body, s.pose.p, s.pose.v);
      while (t < 600 && o.speed < o.circular * 1.01) {
        step(FLIGHT_IDLE);
        o = orbitOf(body, s.pose.p, s.pose.v);
      }
      s.sw[helm.throttle] = 0;
      for (let k = 0; k < 60 * 60; k++) step(FLIGHT_IDLE);
      o = orbitOf(body, s.pose.p, s.pose.v);
      const used = fuel0 - s.get('fuel.kg');
      assert(lowest > 0, 'tocó el suelo');
      assert(s.flight.orbital, 'no pasó a régimen orbital');
      assert(o.orbiting, `no quedó en órbita: periápside ${(o.periapsis / 1000).toFixed(1)} km, apoápside ${(o.apoapsis / 1000).toFixed(1)} km, ${o.speed.toFixed(0)} m/s`);
      assert(s.get('fuel.kg') > 0, 'se quedó sin propelente');
      return `órbita tras ${Math.round(t - burn0 - 60)} s de motor: AP ${(o.apoapsis / 1000).toFixed(1)} km · PE ${(o.periapsis / 1000).toFixed(1)} km · ${o.speed.toFixed(0)} m/s · periodo ${Math.round(o.period / 60)} min · ${used.toFixed(0)} de ${fuel0.toFixed(0)} kg de propelente`;
    },
  },
  {
    name: 'piloto automático orbital (Selene): SUBIR desde la pista llega sola a órbita circular, pasa a CIRCUL. y se desconecta; con SOBREPOT. antes y gastando más',
    run: () => {
      const ascent = (boost: boolean) => {
        const s = sim(HAULER);
        const engines = s.sys.modules.filter((m): m is Engine => m.id.startsWith('engine:'));
        for (const e of engines) {
          s.sw[e.keys.arm] = 1;
          s.sw[e.keys.start] = 1;
        }
        tick(s, 6);
        if (boost) {
          s.sw[`${BOOST.key}.cov`] = 1;
          s.sw[BOOST.key] = 1;
        }
        s.sw['ap.face'] = 1;
        press(s, 'ap.sub');
        const fuel0 = s.get('fuel.kg');
        const t = flyUntil(s, 900, () => s.sw['ap.sub'] !== 1 && s.sw['ap.circ'] !== 1, ground);
        const o = orbitOf(MOON_BODY, s.pose.p, s.pose.v);
        assert(o.orbiting && o.periapsis > 12000, `no quedó en órbita (${boost ? 'con' : 'sin'} sobrepotencia): periápside ${(o.periapsis / 1000).toFixed(1)} km tras ${t.toFixed(0)} s`);
        assert(s.sw['ap.on'] === 1 && s.sw['ap.sub'] === 0 && s.sw['ap.circ'] === 0, 'los modos no terminaron solos');
        return { t, used: fuel0 - s.get('fuel.kg'), o };
      };
      const n = ascent(false);
      const b = ascent(true);
      assert(b.t < n.t * 0.9, `la sobrepotencia no acorta la subida (${b.t.toFixed(0)} s frente a ${n.t.toFixed(0)} s)`);
      assert(b.used > n.used, 'la sobrepotencia no gasta más');
      return `órbita ${(n.o.periapsis / 1000).toFixed(1)}/${(n.o.apoapsis / 1000).toFixed(1)} km en ${n.t.toFixed(0)} s con ${n.used.toFixed(0)} kg · con sobrepotencia ${b.t.toFixed(0)} s y ${b.used.toFixed(0)} kg`;
    },
  },
  {
    name: 'piloto automático orbital (Selene): BAJAR desde una órbita a 20 km con la base 600 km por delante frena, baja y la deja en estacionario sobre la base con NAV',
    run: () => {
      const s = sim(HAULER);
      const engines = s.sys.modules.filter((m): m is Engine => m.id.startsWith('engine:'));
      for (const e of engines) {
        s.sw[e.keys.arm] = 1;
        s.sw[e.keys.start] = 1;
      }
      tick(s, 6);
      airborne(s);
      // circular orbit over the base's meridian, 600 km south of it, heading for it (north)
      const a = 600000 / MOON_BODY.radius;
      const r = MOON_BODY.radius + 20000;
      const vc = Math.sqrt(MOON_BODY.mu / r);
      s.pose.p[0] = 0;
      s.pose.p[1] = MOON_BODY.center[1] + r * Math.cos(a);
      s.pose.p[2] = MOON_BODY.center[2] + r * Math.sin(a);
      s.pose.v[0] = 0;
      s.pose.v[1] = vc * Math.sin(a);
      s.pose.v[2] = -vc * Math.cos(a);
      s.pose.q[0] = Math.sin(a / 2);
      s.pose.q[1] = 0;
      s.pose.q[2] = 0;
      s.pose.q[3] = Math.cos(a / 2);
      s.pose.w.fill(0);
      s.sw['ap.face'] = 1;
      s.sw['ap.on'] = 1;
      s.sw['ap.baj'] = 1;
      const fuel0 = s.get('fuel.kg');
      let lowest = Infinity;
      const t = flyUntil(
        s,
        1200,
        () => {
          lowest = Math.min(lowest, s.flight.agl);
          return s.sw['ap.baj'] !== 1;
        },
        ground,
      );
      const dist = Math.hypot(s.pose.p[0], s.pose.p[2]);
      assert(s.sw['ap.baj'] === 0, `no terminó en ${t.toFixed(0)} s (a ${(dist / 1000).toFixed(1)} km de la base)`);
      assert(dist < 2000, `terminó a ${(dist / 1000).toFixed(1)} km de la base`);
      assert(s.sw['ap.nav'] === 1 && s.sw['ap.face'] === 0, 'no pasó a NAV con la cara SUPERFICIE');
      assert(lowest > 50, `bajó a ${lowest.toFixed(0)} m del suelo`);
      assert(s.get('fuel.kg') > 0, 'se quedó sin propelente');
      return `sobre la base en ${t.toFixed(0)} s: a ${dist.toFixed(0)} m, ${s.flight.agl.toFixed(0)} m de altura, ${(fuel0 - s.get('fuel.kg')).toFixed(0)} kg de propelente`;
    },
  },
  {
    name: 'aterrizar en cualquier parte (Selene): lejos de la base, ATERRIZ. la posa sobre el relieve de la Luna',
    run: () => {
      const s = sim(HAULER);
      const surface = surfaceOf(MOON_BODY, 1969)!;
      const env = { surface };
      const R = MOON_BODY.radius;
      const out: string[] = [];
      for (const km of [777, 1500, 5000]) {
        // on the base's east-west great circle, 120 m over the ground, level on the local horizon
        const a = (km * 1000) / R;
        const d = [Math.sin(a), Math.cos(a), 0];
        const r = R + surface.height(d) + 120;
        for (let i = 0; i < 3; i++) s.pose.p[i] = MOON_BODY.center[i] + d[i] * r;
        s.pose.q[0] = 0;
        s.pose.q[1] = 0;
        s.pose.q[2] = Math.sin(-a / 2);
        s.pose.q[3] = Math.cos(-a / 2);
        s.pose.v.fill(0);
        s.pose.w.fill(0);
        s.landed = false;
        s.onPad = false;
        s.flight.wake();
        s.sw['ap.face'] = 0;
        s.sw['ap.on'] = 1;
        s.sw['ap.land'] = 1;
        let t = 0;
        let acc = 0;
        while (t < 150 && !(s.sw['ap.on'] === 0 && t > 5)) {
          s.flight.step(1 / 60, FLIGHT_IDLE, env);
          t += 1 / 60;
          acc += 1 / 60;
          while (acc >= 1 / SYSTEMS_HZ) {
            acc -= 1 / SYSTEMS_HZ;
            s.tick(1 / SYSTEMS_HZ);
          }
        }
        for (let i = 0; i < 120; i++) s.flight.step(1 / 60, FLIGHT_IDLE, env);
        assert(s.landed, `a ${km} km de la base no se posó en ${t.toFixed(0)} s (a ${s.flight.agl.toFixed(1)} m del suelo)`);
        assert(Math.abs(s.flight.agl) < 0.5, `a ${km} km quedó a ${s.flight.agl.toFixed(2)} m del suelo`);
        out.push(`${km} km en ${t.toFixed(0)} s`);
      }
      return `posada ${out.join(' · ')}`;
    },
  },
  {
    name: 'vuelo: sin fuerzas mágicas — lo que cambia la velocidad y el giro es exactamente el empuje de las toberas más la gravedad',
    run: () => {
      const s = sim(HAULER);
      airborne(s);
      tick(s, 1);
      const fm = s.flight;
      const T = fm.thrusters;
      let worstF = 0;
      let worstT = 0;
      let turned = 0;
      const cmds: FlightCommand[] = [
        { ...FLIGHT_IDLE, surge: 1, yaw: 0.6 },
        { ...FLIGHT_IDLE, heave: -0.5, roll: 1 },
        { ...FLIGHT_IDLE, sway: 1, pitch: -0.7 },
      ];
      for (const cmd of cmds) {
        for (let i = 0; i < 120; i++) {
          if (i % 3 === 0) s.tick(1 / SYSTEMS_HZ);
          const q0 = [...s.pose.q] as [number, number, number, number];
          const wb0 = qRotate(qConj(q0), s.pose.w);
          const mp = s.massNow();
          const comW = qRotate(q0, mp.com);
          const v0 = [s.pose.v[0] + s.pose.w[1] * comW[2] - s.pose.w[2] * comW[1], s.pose.v[1] + s.pose.w[2] * comW[0] - s.pose.w[0] * comW[2], s.pose.v[2] + s.pose.w[0] * comW[1] - s.pose.w[1] * comW[0]];
          fm.step(1 / 60, cmd, { surface: SKY });
          // thrust from the outputs the model published (after its lags), rebuilt independently
          const F: V3 = [0, 0, 0];
          const Tq: V3 = [0, 0, 0];
          const push = (f: V3, at: V3) => {
            const r = [at[0] - mp.com[0], at[1] - mp.com[1], at[2] - mp.com[2]];
            F[0] += f[0];
            F[1] += f[1];
            F[2] += f[2];
            Tq[0] += r[1] * f[2] - r[2] * f[1];
            Tq[1] += r[2] * f[0] - r[0] * f[2];
            Tq[2] += r[0] * f[1] - r[1] * f[0];
          };
          T.mains.forEach((t, k) => push(scale(t.dir, fm.out[k] * t.maxN), t.at));
          T.lifts.forEach((t, k) => {
            const [a, b, c] = [fm.liftVec[k * 3], fm.liftVec[k * 3 + 1], fm.liftVec[k * 3 + 2]];
            const e1 = Math.abs(t.dir[1]) < 0.9 ? norm3([t.dir[2], 0, -t.dir[0]]) : norm3([0, -t.dir[2], t.dir[1]]);
            const e2: V3 = [t.dir[1] * e1[2] - t.dir[2] * e1[1], t.dir[2] * e1[0] - t.dir[0] * e1[2], t.dir[0] * e1[1] - t.dir[1] * e1[0]];
            push([(t.dir[0] * a + e1[0] * b + e2[0] * c) * t.maxN, (t.dir[1] * a + e1[1] * b + e2[1] * c) * t.maxN, (t.dir[2] * a + e1[2] * b + e2[2] * c) * t.maxN], t.at);
          });
          const DIRS: V3[] = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];
          T.rcs.forEach((t, k) => DIRS.forEach((d, c) => push(scale(d, fm.rcsOut[k * 6 + c] * t.maxN), t.at)));
          const Fw = qRotate(q0, F);
          const comW1 = qRotate(s.pose.q, mp.com);
          const v1 = [s.pose.v[0] + s.pose.w[1] * comW1[2] - s.pose.w[2] * comW1[1], s.pose.v[1] + s.pose.w[2] * comW1[0] - s.pose.w[0] * comW1[2], s.pose.v[2] + s.pose.w[0] * comW1[1] - s.pose.w[1] * comW1[0]];
          // gravity: the body's pull at the centre of mass, as the model applied it this step
          const gv = fm.gravity;
          const expect = [(Fw[0] / mp.mass + gv[0]) / 60, (Fw[1] / mp.mass + gv[1]) / 60, (Fw[2] / mp.mass + gv[2]) / 60];
          worstF = Math.max(worstF, Math.hypot(v1[0] - v0[0] - expect[0], v1[1] - v0[1] - expect[1], v1[2] - v0[2] - expect[2]));
          // Euler: I·dω/dt = T − ω × Iω, in ship axes
          const gyro = cross(wb0, mulM3(tensor(mp.inertia), wb0));
          const dw = mulM3(mp.inv, [Tq[0] - gyro[0], Tq[1] - gyro[1], Tq[2] - gyro[2]]);
          const wb1 = qRotate(qConj(s.pose.q), s.pose.w);
          worstT = Math.max(worstT, Math.hypot(wb1[0] - wb0[0] - dw[0] / 60, wb1[1] - wb0[1] - dw[1] / 60, wb1[2] - wb0[2] - dw[2] / 60));
          turned = Math.max(turned, Math.hypot(...wb1));
        }
      }
      assert(worstF < 1e-6, `la velocidad cambia ${worstF.toExponential(2)} m/s más de lo que empujan las toberas`);
      assert(worstT < 1e-6, `el giro cambia ${worstT.toExponential(2)} rad/s más de lo que da el par de las toberas`);
      assert(!s.landed, 'tocó el suelo');
      assert(turned > 0.05, `la prueba no ha girado la nave (${turned.toFixed(3)} rad/s)`);
      return `giro hasta ${turned.toFixed(2)} rad/s · desvío máximo ${worstF.toExponential(1)} m/s y ${worstT.toExponential(1)} rad/s por paso en 6 s de maniobras`;
    },
  },
  {
    name: 'vuelo: despegue y aterrizaje automáticos (todas las naves) — tren arriba y abajo solo, se posa nivelada y se desconecta',
    run: () => {
      const out: string[] = [];
      for (const def of SHIPS) {
        const s = sim(def, 0.3, BUMPY);
        fly(s, 1, FLIGHT_IDLE, BUMPY);
        s.sw['ap.alt.sel'] = def.autopilot!.alts.indexOf(20);
        press(s, 'ap.to');
        const up = flyUntil(s, 40, () => s.sw['ap.to'] === 0, BUMPY);
        assert(up < 40, `${def.id}: el despegue no termina (AGL ${s.flight.agl.toFixed(1)} m)`);
        assert(s.sw['ap.alt'] === 1 && s.sw['ap.lvl'] === 1, `${def.id}: tras despegar no mantiene altura y nivel`);
        const gearKey = def.gear!.key;
        fly(s, 8, FLIGHT_IDLE, BUMPY);
        assert(s.sw[gearKey] === 0, `${def.id}: el tren no sube solo`);
        assert(Math.abs(s.flight.agl - 20) < 1.5, `${def.id}: no se queda a 20 m (${s.flight.agl.toFixed(2)} m)`);
        press(s, 'ap.land');
        const down = flyUntil(s, 80, () => s.sw['ap.on'] === 0, BUMPY);
        assert(down < 80, `${def.id}: no termina de aterrizar (AGL ${s.flight.agl.toFixed(2)} m, modos ${s.flight.telemetry.modes.join('/')})`);
        assert(s.sw[gearKey] === 1 && s.landed, `${def.id}: tren ${s.sw[gearKey]}, en tierra ${s.landed}`);
        assert(levelness(s.pose.q) > 0.995, `${def.id}: se posa inclinada`);
        assert(!def.autopilot!.modes.some((m) => s.sw[`ap.${m}`] === 1), `${def.id}: quedan modos conectados en tierra`);
        out.push(`${def.name}: arriba en ${up.toFixed(0)} s, abajo en ${down.toFixed(0)} s`);
      }
      return out.join(' · ');
    },
  },
  {
    name: 'vuelo: NAV lleva la nave al punto elegido, pone rumbo y se queda encima en estacionario',
    run: () => {
      const s = sim(PEREGRINA);
      fly(s, 1, FLIGHT_IDLE, BUMPY);
      const target = NAV_POINTS.findIndex((p) => p.id === 'base');
      s.sw['ap.wp'] = target;
      s.sw['ap.alt.sel'] = PEREGRINA.autopilot!.alts.indexOf(20);
      s.pose.p[0] = 180;
      s.pose.p[2] = -140;
      s.pose.p[1] = at(BUMPY).height(180, -140) + PEREGRINA.floorHeight;
      s.flight.wake();
      press(s, 'ap.to');
      flyUntil(s, 40, () => s.sw['ap.to'] === 0, BUMPY);
      press(s, 'ap.nav');
      assert(s.sw['ap.alt'] === 1 && s.sw['ap.hdg'] === 0, 'NAV no deja la altura o no quita el rumbo');
      const t = flyUntil(s, 120, () => s.sw['ap.nav'] === 0, BUMPY);
      const wp = NAV_POINTS[target];
      const d = bearingTo(MOON_BODY, s.pose.p, wp).dist;
      assert(t < 120 && d < 8, `no llega (a ${d.toFixed(1)} m tras ${t.toFixed(0)} s)`);
      fly(s, 6, FLIGHT_IDLE, BUMPY);
      const v = Math.hypot(s.pose.v[0], s.pose.v[2]);
      assert(v < 0.3 && Math.abs(s.flight.agl - 20) < 1.5, `no se queda encima (v ${v.toFixed(2)} m/s, AGL ${s.flight.agl.toFixed(1)} m)`);
      return `${Math.hypot(180, 140).toFixed(0)} m en ${t.toFixed(0)} s, a ${d.toFixed(1)} m del punto`;
    },
  },
  {
    name: 'piloto automático: exclusión de modos, general apagado limpia todo, sin aviónica se desconecta y el mando pasa a DIRECTO',
    run: () => {
      const s = sim(HAULER);
      airborne(s);
      s.sw['ap.wp'] = NAV_POINTS.findIndex((p) => p.id === 'crater');
      press(s, 'ap.hdg');
      tick(s, 0.1);
      assert(s.sw['ap.on'] === 1, 'pulsar un modo no conecta el general');
      press(s, 'ap.nav');
      tick(s, 0.1);
      assert(s.sw['ap.hdg'] === 0 && s.sw['ap.nav'] === 1, 'NAV no quita RUMBO');
      press(s, 'ap.lvl');
      tick(s, 0.1);
      press(s, 'ap.on');
      tick(s, 0.1);
      assert(!HAULER.autopilot!.modes.some((m) => s.sw[`ap.${m}`] === 1), 'apagar el general deja modos conectados');
      press(s, 'ap.alt');
      tick(s, 0.1);
      // the flight computer loses its circuit
      const brk = HAULER.subsystems.find((c) => c.id === HAULER.autopilot!.circuit)!.breaker;
      s.sw[brk] = 0;
      const said: string[] = [];
      for (let i = 0; i < 10; i++) for (const e of s.tick(1 / SYSTEMS_HZ).events) if (e.type === 'say') said.push(e.text);
      assert(s.sw['ap.on'] === 0 && s.sw['ap.alt'] === 0, 'sin aviónica sigue conectado');
      assert(said.some((t) => t.includes('desconectado')), 'no avisa de la desconexión');
      const refused = s.interact(ctl(s, 'ck.ap/ap.alt').index);
      assert('reason' in refused, 'deja conectar un modo sin aviónica');
      fly(s, 0.5, FLIGHT_IDLE, SKY);
      assert(s.flight.telemetry.direct, 'no pasa a mando DIRECTO');
      const vy = s.pose.v[1];
      fly(s, 2, FLIGHT_IDLE, SKY);
      assert(s.pose.v[1] < vy - 2.5, `en DIRECTO sigue compensando la gravedad (${vy.toFixed(2)} → ${s.pose.v[1].toFixed(2)} m/s)`);
      return `${said[0]} · DIRECTO cae ${(vy - s.pose.v[1]).toFixed(1)} m/s en 2 s`;
    },
  },
  {
    name: 'pose: mundo ↔ nave de ida y vuelta; un punto a bordo sigue a la nave; la interpolación de red pasa por las muestras',
    run: () => {
      const s = sim(PEREGRINA, 0.7);
      const p: V3 = [1.2, 0.4, -3];
      const w = s.toWorld(p);
      const back = s.toLocal(w);
      assert(Math.hypot(back[0] - p[0], back[1] - p[1], back[2] - p[2]) < 1e-9, `ida y vuelta ${back}`);
      const c = Math.cos(0.7);
      const sn = Math.sin(0.7);
      const legacy = [p[0] * c + p[2] * sn + s.place.p[0], p[1] + s.place.p[1], -p[0] * sn + p[2] * c + s.place.p[2]];
      assert(Math.hypot(w[0] - legacy[0], w[1] - legacy[1], w[2] - legacy[2]) < 1e-9, `distinto del giro por guiñada ${w} vs ${legacy}`);
      // a point fixed on the deck is carried rigidly through a manoeuvre
      airborne(s);
      const prev = clonePose(s.pose);
      fly(s, 1, { ...FLIGHT_IDLE, yaw: 1, surge: 1, roll: 0.5 }, SKY);
      const moved = s.toLocal(carried(prev, s.pose, toWorld(prev, p)));
      assert(Math.hypot(moved[0] - p[0], moved[1] - p[1], moved[2] - p[2]) < 1e-9, `el punto a bordo se soltó ${moved}`);
      // network playback: Hermite passes through both samples and keeps the speed between them
      const a = clonePose(s.pose);
      fly(s, 0.1, { ...FLIGHT_IDLE, surge: 1 }, SKY);
      const b = clonePose(s.pose);
      const h0 = hermitePose(a, b, 0, 0.1);
      const h1 = hermitePose(a, b, 1, 0.1);
      const mid = hermitePose(a, b, 0.5, 0.1);
      const lin = lerpPose(a, b, 0.5);
      assert(Math.hypot(h0.p[0] - a.p[0], h0.p[1] - a.p[1], h0.p[2] - a.p[2]) < 1e-9 && Math.hypot(h1.p[0] - b.p[0], h1.p[1] - b.p[1], h1.p[2] - b.p[2]) < 1e-9, 'Hermite no pasa por las muestras');
      assert(Math.hypot(mid.p[0] - lin.p[0], mid.p[1] - lin.p[1], mid.p[2] - lin.p[2]) < 0.05, 'Hermite se aparta de la trayectoria');
      return 'cuaternión = guiñada para naves aparcadas; a bordo = rígido; Hermite ok';
    },
  },
  {
    name: 'mentón: los focos de aterrizaje están sobre su cara inclinada',
    run: () => {
      let n = 0;
      for (const def of SHIPS) {
        const chin = def.props.find((p) => p.model === 'chin');
        if (!chin) continue;
        for (const l of def.extLights.filter((x) => x.kind === 'landing')) {
          const lz = (l.pos[2] - chin.c[2]) / chin.half[2];
          const ly = (l.pos[1] - chin.c[1]) / chin.half[1];
          const [a, b] = [PROFILES.chin[2], PROFILES.chin[3]];
          const off = (b[0] - a[0]) * (ly - a[1]) - (b[1] - a[1]) * (lz - a[0]);
          assert(Math.abs(off) < 1e-3, `${def.id}: un foco no está sobre la cara del mentón (${off.toFixed(4)})`);
          n++;
        }
      }
      return `${n} focos`;
    },
  },
];

for (let i = 0; i < jobs.length; i++) {
  const pct = Math.round((i / jobs.length) * 100);
  const job = jobs[i];
  process.stdout.write(`[${String(pct).padStart(3, ' ')}%] ${job.name}\n`);
  try {
    const detail = job.run();
    process.stdout.write(`       ok — ${detail}\n`);
  } catch (e) {
    failed++;
    process.stdout.write(`       FALLO — ${e instanceof Error ? e.message : String(e)}\n`);
  }
}

const snap = fresh();
process.stdout.write(
  `[100%] ${failed === 0 ? 'todo en orden' : `${failed} fallo(s)`} — reactor ${snap.sw.reactor ? 'en marcha' : 'parado'}, salida ${snap.sw['rx.set']}, bodega ${snap.sys.pressure(snap.st, 'cargo').toFixed(2)} kPa, página ${activePage(HAULER.screens[0], snap.sw)}\n`,
);
process.exit(failed === 0 ? 0 : 1);
