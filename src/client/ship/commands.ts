import type { ConsoleCommand } from '../../shared/screens';

/** Event routing for station controls. New adapters register a namespace, never a ship name. */
export class ConsoleCommands<Host> {
  private handlers = new Map<string, (host: Host, command: ConsoleCommand) => string | null>();

  register(namespace: string, handler: (host: Host, command: ConsoleCommand) => string | null) {
    if (this.handlers.has(namespace)) throw new Error(`console command namespace already registered: ${namespace}`);
    this.handlers.set(namespace, handler);
  }

  execute(host: Host, command: ConsoleCommand): string | null {
    const handler = this.handlers.get(command.namespace);
    return handler ? handler(host, command) : 'Este puesto no tiene un adaptador de mandos';
  }
}
