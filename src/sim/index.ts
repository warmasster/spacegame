// The world simulation core (docs/MUNDO.md): pure, headless, platform-free. Hosts (the Node worker,
// a browser Worker) live in ./host and import from here, never the other way round.

export { Calendar, WorldClock, STANDARD_CALENDAR, MINUTE, HOUR, type CalendarDef, type DateParts } from './core/calendar.js';
export { Chronicle, DEFAULT_CHUNK, type LogRecord } from './core/chronicle.js';
export { approachAt, approachWhen, compoundAt, linearAt, linearFields, linearWhen, LinearField } from './core/lazy.js';
export { EventQueue } from './core/queue.js';
export { draw, hash2, hash3, mix32, Rng, seedOf, stagger, unit } from './core/rng.js';
export { defineComponent, Store, Table, type Column, type ComponentDef, type FieldSpec, type FieldType, type Fields, type TableData } from './core/store.js';
export { World, type EventDef, type RecordOptions, type SimEvent, type SimModule, type WorldInit } from './core/world.js';
export { MemoryStorage, WorldSaves, SAVE_SLOTS, segmentName, type Loaded, type SaveInfo, type SaveStorage } from './persist/saves.js';
export { digest, readHeader, restore, snapshot, FORMAT, type Header, type RestoreReport, type Snapshot } from './persist/snapshot.js';
