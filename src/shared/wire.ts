// Binary form of the messages that go out many times a second: player states and snapshots, ship
// poses, the pilot's flight reports and ship state diffs. Little-endian; float32 wherever float64's
// extra digits are noise at game scale (a millimetre at 16 km). A pose goes from ~195 bytes of
// JSON to 76, a player's state upload to 59 + its UTF-8 equipment id (the time of its step as
// float64: a millisecond is 1.6 m in orbit). Everything else stays JSON: rare, and easy to read
// in a log. Both ends call `encode*` first and fall back to JSON when it returns null.

import type { ClientMessage, PlayerState, PoseWire, ServerMessage } from './protocol.js';

const TAG = { state: 1, snapshot: 2, shipPose: 3, shipSt: 4, flight: 5, npcs: 6 } as const;

/** Positions go as float64 (a ship in orbit is millions of metres out: float32 would step by 12 cm there). */
// Fixed fields + UTF-8 byte count; the catalog id itself is variable length. Never encode a
// weapon as its number-key slot: adding/reordering a catalog must not change its identity.
const STATE_BYTES = 50;
const POSE_BYTES = 65;
const encoder = new TextEncoder();
const decoder = new TextDecoder();
const noId = new Uint8Array(0);
const ids = new Map<string, Uint8Array>();

/** Bounded cache: normal equipment ids are encoded once, not on every state upload. */
function stateId(s: PlayerState): Uint8Array {
  if (!s.w) return noId;
  let bytes = ids.get(s.w);
  if (!bytes) {
    bytes = encoder.encode(s.w);
    if (bytes.length > 65535) throw new RangeError('equipment id exceeds wire length');
    if (ids.size >= 256) ids.clear();
    ids.set(s.w, bytes);
  }
  return bytes;
}

class Out {
  readonly buf: ArrayBuffer;
  readonly v: DataView;
  o = 0;
  constructor(n: number) {
    this.buf = new ArrayBuffer(n);
    this.v = new DataView(this.buf);
  }
  u8(x: number) {
    this.v.setUint8(this.o, x);
    this.o += 1;
  }
  u16(x: number) {
    this.v.setUint16(this.o, x, true);
    this.o += 2;
  }
  u32(x: number) {
    this.v.setUint32(this.o, x, true);
    this.o += 4;
  }
  i16(x: number) {
    this.v.setInt16(this.o, x, true);
    this.o += 2;
  }
  f32(x: number) {
    this.v.setFloat32(this.o, x, true);
    this.o += 4;
  }
  f64(x: number) {
    this.v.setFloat64(this.o, x, true);
    this.o += 8;
  }
  bytes(x: Uint8Array) {
    new Uint8Array(this.buf, this.o, x.length).set(x);
    this.o += x.length;
  }
}

class In {
  o = 0;
  constructor(readonly v: DataView) {}
  u8() {
    return this.v.getUint8(this.o++);
  }
  u16() {
    const x = this.v.getUint16(this.o, true);
    this.o += 2;
    return x;
  }
  u32() {
    const x = this.v.getUint32(this.o, true);
    this.o += 4;
    return x;
  }
  i16() {
    const x = this.v.getInt16(this.o, true);
    this.o += 2;
    return x;
  }
  f32() {
    const x = this.v.getFloat32(this.o, true);
    this.o += 4;
    return x;
  }
  f64() {
    const x = this.v.getFloat64(this.o, true);
    this.o += 8;
    return x;
  }
  text(n: number) {
    if (this.o + n > this.v.byteLength) throw new RangeError('truncated equipment id');
    const s = decoder.decode(new Uint8Array(this.v.buffer, this.v.byteOffset + this.o, n));
    this.o += n;
    return s;
  }
}

function putState(w: Out, s: PlayerState) {
  w.f64(s.p[0]);
  w.f64(s.p[1]);
  w.f64(s.p[2]);
  w.f32(s.v[0]);
  w.f32(s.v[1]);
  w.f32(s.v[2]);
  w.f32(s.yaw);
  w.f32(s.pitch);
  w.u16(s.f);
  w.i16(s.fr ?? -1);
  const id = stateId(s);
  w.u16(id.length);
  w.bytes(id);
}

function getState(r: In): PlayerState {
  const p: [number, number, number] = [r.f64(), r.f64(), r.f64()];
  const v: [number, number, number] = [r.f32(), r.f32(), r.f32()];
  const yaw = r.f32();
  const pitch = r.f32();
  const f = r.u16();
  const fr = r.i16();
  const w = r.text(r.u16());
  const s: PlayerState = { p, v, yaw, pitch, f };
  if (fr >= 0) s.fr = fr;
  if (w) s.w = w;
  return s;
}

function putPose(w: Out, p: PoseWire) {
  for (let i = 0; i < 3; i++) w.f64(p.p[i]);
  for (let i = 0; i < 4; i++) w.f32(p.q[i]);
  for (let i = 0; i < 3; i++) w.f32(p.v[i]);
  for (let i = 0; i < 3; i++) w.f32(p.w[i]);
  w.u8((p.landed ? 1 : 0) | (p.pad ? 2 : 0));
}

function getPose(r: In): PoseWire {
  const p: [number, number, number] = [r.f64(), r.f64(), r.f64()];
  const q: [number, number, number, number] = [r.f32(), r.f32(), r.f32(), r.f32()];
  const v: [number, number, number] = [r.f32(), r.f32(), r.f32()];
  const w: [number, number, number] = [r.f32(), r.f32(), r.f32()];
  const fl = r.u8();
  return { p, q, v, w, landed: (fl & 1) !== 0, pad: (fl & 2) !== 0 };
}

/** A server message in binary, or null (send it as JSON). */
export function encodeServer(msg: ServerMessage): ArrayBuffer | null {
  switch (msg.type) {
    case 'snapshot': {
      let size = 1 + 8 + 2;
      for (const state of msg.states) size += 2 + 8 + STATE_BYTES + stateId(state.s).length;
      const w = new Out(size);
      w.u8(TAG.snapshot);
      w.f64(msg.t);
      w.u16(msg.states.length);
      for (const s of msg.states) {
        w.u16(s.id);
        w.f64(s.t);
        putState(w, s.s);
      }
      return w.buf;
    }
    case 'npcs': {
      // like a snapshot, with world ids (u32)
      let size = 1 + 8 + 2;
      for (const state of msg.states) size += 4 + 8 + STATE_BYTES + stateId(state.s).length;
      const w = new Out(size);
      w.u8(TAG.npcs);
      w.f64(msg.t);
      w.u16(msg.states.length);
      for (const s of msg.states) {
        w.u32(s.id);
        w.f64(s.t);
        putState(w, s.s);
      }
      return w.buf;
    }
    case 'shipPose': {
      const w = new Out(1 + 2 + 8 + POSE_BYTES);
      w.u8(TAG.shipPose);
      w.u16(msg.ship);
      w.f64(msg.t);
      putPose(w, msg);
      return w.buf;
    }
    case 'shipSt': {
      const n = msg.d.length >> 1;
      const w = new Out(1 + 2 + 2 + n * 6);
      w.u8(TAG.shipSt);
      w.u16(msg.ship);
      w.u16(n);
      for (let k = 0; k < n; k++) {
        w.u16(msg.d[2 * k]);
        w.f32(msg.d[2 * k + 1]);
      }
      return w.buf;
    }
    default:
      return null;
  }
}

/** A client message in binary, or null (send it as JSON). */
export function encodeClient(msg: ClientMessage): ArrayBuffer | null {
  switch (msg.type) {
    case 'state': {
      const w = new Out(1 + 8 + STATE_BYTES + stateId(msg.s).length);
      w.u8(TAG.state);
      w.f64(msg.t);
      putState(w, msg.s);
      return w.buf;
    }
    case 'flight': {
      const w = new Out(1 + 2 + 8 + 4 + POSE_BYTES + 2 + msg.out.length * 4);
      w.u8(TAG.flight);
      w.u16(msg.ship);
      w.f64(msg.t);
      w.f32(msg.agl);
      putPose(w, msg);
      w.u16(msg.out.length);
      for (const x of msg.out) w.f32(x);
      return w.buf;
    }
    default:
      return null;
  }
}

/** A binary server message (null: unknown tag). */
export function decodeServer(v: DataView): ServerMessage | null {
  try {
    return readServer(v);
  } catch (e) {
    if (e instanceof RangeError) return null;
    throw e;
  }
}

function readServer(v: DataView): ServerMessage | null {
  const r = new In(v);
  switch (r.u8()) {
    case TAG.snapshot: {
      const t = r.f64();
      const n = r.u16();
      const states: Array<{ id: number; t: number; s: PlayerState }> = [];
      for (let k = 0; k < n; k++) {
        const id = r.u16();
        const ts = r.f64();
        states.push({ id, t: ts, s: getState(r) });
      }
      return { type: 'snapshot', t, states };
    }
    case TAG.npcs: {
      const t = r.f64();
      const n = r.u16();
      const states: Array<{ id: number; t: number; s: PlayerState }> = [];
      for (let k = 0; k < n; k++) {
        const id = r.u32();
        const ts = r.f64();
        states.push({ id, t: ts, s: getState(r) });
      }
      return { type: 'npcs', t, states };
    }
    case TAG.shipPose: {
      const ship = r.u16();
      const t = r.f64();
      return { type: 'shipPose', ship, t, ...getPose(r) };
    }
    case TAG.shipSt: {
      const ship = r.u16();
      const n = r.u16();
      const d: number[] = [];
      for (let k = 0; k < n; k++) d.push(r.u16(), r.f32());
      return { type: 'shipSt', ship, d };
    }
    default:
      return null;
  }
}

/** A binary client message (null: unknown tag). */
export function decodeClient(v: DataView): ClientMessage | null {
  try {
    return readClient(v);
  } catch (e) {
    if (e instanceof RangeError) return null;
    throw e;
  }
}

function readClient(v: DataView): ClientMessage | null {
  const r = new In(v);
  switch (r.u8()) {
    case TAG.state: {
      const t = r.f64();
      return { type: 'state', t, s: getState(r) };
    }
    case TAG.flight: {
      const ship = r.u16();
      const t = r.f64();
      const agl = r.f32();
      const pose = getPose(r);
      const n = r.u16();
      const out: number[] = [];
      for (let k = 0; k < n; k++) out.push(r.f32());
      return { type: 'flight', ship, t, agl, out, ...pose };
    }
    default:
      return null;
  }
}
