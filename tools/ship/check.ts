/**
 * Fast ship check: the rules, without a browser. Prints a percent before each job and exits 1
 * on the first failed assertion inside a job (the rest still run).
 *
 *   npm run test:ship
 */
import { activePage, buildParts, checkShip, controlHit, coverHit, finishShip, nacellePylons, ShipBuilder, shutCovers, type ManualSection } from '../../src/shared/ship/def.js';
import { crewStep } from '../../src/shared/ship/crew.js';
import type { SystemFactory } from '../../src/shared/ship/modules/api.js';
import { ENG } from '../../src/shared/ship/modules/apu.js';
import { SYSTEM_FACTORIES } from '../../src/shared/ship/modules/index.js';
import { ReasonHold } from '../../src/shared/ship/hold.js';
import { madd, rayBox, scale } from '../../src/shared/ship/geom.js';
import { HAULER, PEREGRINA, SHIP_DEFS } from '../../src/shared/ship/ships/index.js';
import { allComponents, allFurniture, component, componentSheet, defineComponent, part, PROFILES } from '../../src/shared/ship/catalog/index.js';
import { engineFractions, integrate, massProperties, performance, thrust, thrusters, type ShipPose } from '../../src/shared/ship/flight.js';
import { LOCK } from '../../src/shared/ship/modules/airlock.js';
import { qYaw } from '../../src/shared/ship/geom.js';
import type { ShipDef } from '../../src/shared/ship/def.js';
import { ShipSim, placeShip, SYSTEMS_HZ } from '../../src/shared/ship/sim.js';
import { RX } from '../../src/shared/ship/modules/reactor.js';

const ground = { height: () => 0 };
let failed = 0;

const SHIPS: ShipDef[] = Object.values(SHIP_DEFS);

function sim(def: ShipDef, yaw = 0) {
  return new ShipSim(1, def, placeShip(def, 0, 0, yaw, ground), ground);
}

function fresh() {
  return sim(HAULER);
}

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
      const sim = new ShipSim(9, def, placeShip(def, 0, 0, 0, ground), ground);
      const ids = sim.sys.modules.map((m) => m.id);
      assert(!ids.some((i) => i.startsWith('reactor') || i.startsWith('engine')), `módulos de más: ${ids.join(', ')}`);
      tick(sim, 60);
      const soc = sim.get('bat.soc');
      assert(sim.powered('main') && soc < 0.92, `circuito ${sim.powered('main')}, batería ${soc}`);
      assert(sim.sys.pressure(sim.st, 'cab') > 60, `cabina a ${sim.sys.pressure(sim.st, 'cab').toFixed(1)} kPa`);
      let refused = false;
      try {
        finishShip({ ...def, parts: buildParts([{ id: 'x', type: 'warpcore', name: 'X', c: [0, 0, 0], half: [0.1, 0.1, 0.1], zone: 'cab', maxHp: 1 }]) });
        new ShipSim(10, { ...def, parts: buildParts([{ id: 'x', type: 'warpcore', name: 'X', c: [0, 0, 0], half: [0.1, 0.1, 0.1], zone: 'cab', maxHp: 1 }]) }, placeShip(def, 0, 0, 0, ground), ground);
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
        const sim = new ShipSim(11, def, placeShip(def, 0, 0, 0, ground), ground);
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
      const sim = new ShipSim(12, def, placeShip(def, 0, 0, 0, ground), ground);
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
    name: 'acelerador: el motor sigue al acelerador y gasta propelente en proporción',
    run: () => {
      const s = sim(PEREGRINA);
      for (const id of ['ck.l/guard.eng', 'ck.l/eng.arm', 'ck.l/eng.start']) {
        const r = s.interact(ctl(s, id).index);
        assert(!('reason' in r), `${id}: ${'reason' in r ? r.reason : ''}`);
        tick(s, 0.2);
      }
      tick(s, 4);
      assert(s.get('eng.state') === ENG.run, `motor en estado ${s.get('eng.state')}`);
      const idle = s.get('fuel.kg');
      tick(s, 10);
      const idleUse = idle - s.get('fuel.kg');
      const knob = ctl(s, 'ck.main/helm.throttle');
      for (let i = 0; i < 10; i++) s.interact(knob.index, 1);
      tick(s, 4);
      assert(s.get('eng.thr') > 0.95, `empuje ${s.get('eng.thr')}`);
      const full = s.get('fuel.kg');
      tick(s, 10);
      const fullUse = full - s.get('fuel.kg');
      assert(fullUse > idleUse * 8 && Math.abs(fullUse - 10) < 1, `ralentí ${idleUse.toFixed(2)} kg, a tope ${fullUse.toFixed(2)} kg en 10 s`);
      const F = thrust(thrusters(PEREGRINA), s.massNow().com, engineFractions(PEREGRINA, (k) => s.get(k))).F;
      assert(F[2] < -28000, `fuerza del motor ${F.map((v) => v.toFixed(0))}`);
      return `ralentí ${idleUse.toFixed(2)} kg · 100 % ${fullUse.toFixed(1)} kg en 10 s · empuje ${(-F[2] / 1000).toFixed(1)} kN`;
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
  // --- flight groundwork -------------------------------------------------------------------------
  {
    name: 'vuelo (preparación): masa, centro de masas, empuje; el integrador acelera recto y gira con par',
    run: () => {
      const out: string[] = [];
      for (const def of SHIPS) {
        const m = massProperties(def);
        const perf = performance(def, m, 1.62);
        assert(m.mass > 3000 && m.mass < 60000, `${def.id}: masa ${m.mass.toFixed(0)} kg`);
        assert(Math.abs(m.com[0]) < 0.4, `${def.id}: centro de masas desplazado ${m.com[0].toFixed(2)} m a un lado`);
        assert(perf.twr > 1.2, `${def.id}: empuje/peso lunar ${perf.twr.toFixed(2)}`);
        assert(m.inertia[0] > 0 && m.inertia[1] > 0 && m.inertia[2] > 0, `${def.id}: inercia ${m.inertia}`);
        out.push(`${def.name} ${(m.mass / 1000).toFixed(1)} t, TWR ${perf.twr.toFixed(2)}, Δv ${perf.dv.toFixed(0)} m/s`);
      }
      // thrust through the centre of mass: accelerates along −z, no spin
      const m = massProperties(PEREGRINA);
      const pose: ShipPose = { p: [0, 0, 0], q: qYaw(0), v: [0, 0, 0], w: [0, 0, 0] };
      for (let i = 0; i < 60; i++) integrate(pose, m, [0, 0, -m.mass], [0, 0, 0], [0, 0, 0], 1 / 60);
      assert(Math.abs(pose.v[2] + 1) < 1e-6 && Math.abs(pose.p[2] + 0.5) < 0.02 && Math.hypot(...pose.w) === 0, `recta: v ${pose.v} p ${pose.p}`);
      // a torque about +y turns the nose to port (yaw grows)
      const spin: ShipPose = { p: [0, 0, 0], q: qYaw(0), v: [0, 0, 0], w: [0, 0, 0] };
      for (let i = 0; i < 60; i++) integrate(spin, m, [0, 0, 0], [0, m.inertia[1], 0], [0, 0, 0], 1 / 60);
      assert(Math.abs(spin.w[1] - 1) < 1e-6 && spin.q[1] > 0.2, `giro: w ${spin.w} q ${spin.q}`);
      return out.join(' · ');
    },
  },
  {
    name: 'pose: mundo ↔ nave de ida y vuelta, igual que el giro por guiñada de antes',
    run: () => {
      const s = sim(PEREGRINA, 0.7);
      const p: [number, number, number] = [1.2, 0.4, -3];
      const w = s.toWorld(p);
      const back = s.toLocal(w);
      assert(Math.hypot(back[0] - p[0], back[1] - p[1], back[2] - p[2]) < 1e-9, `ida y vuelta ${back}`);
      const c = Math.cos(0.7);
      const sn = Math.sin(0.7);
      const legacy = [p[0] * c + p[2] * sn + s.place.x, p[1] + s.place.y, -p[0] * sn + p[2] * c + s.place.z];
      assert(Math.hypot(w[0] - legacy[0], w[1] - legacy[1], w[2] - legacy[2]) < 1e-9, `distinto del giro por guiñada ${w} vs ${legacy}`);
      return 'cuaternión = guiñada para naves aparcadas';
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
