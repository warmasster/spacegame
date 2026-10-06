import type { PipSource } from './pip';

/** Providers are adapters. The renderer does not know what "mount", "mirror" or "TV" means. */
export class CameraSources<Context> {
  private factories = new Map<string, (context: Context) => PipSource>();
  register(kind: string, factory: (context: Context) => PipSource) {
    if (this.factories.has(kind)) throw new Error(`camera provider already registered: ${kind}`);
    this.factories.set(kind, factory);
  }
  create(kind: string, context: Context): PipSource {
    const factory = this.factories.get(kind);
    if (!factory) throw new Error(`camera provider not registered: ${kind}`);
    return factory(context);
  }
}
