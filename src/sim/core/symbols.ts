// Interned strings (`sym` fields, record kinds): a name is stored once and referred to by number.
// Numbers are given in order of first use and saved in that order, so they are the same after a load.

export class Symbols {
  private names: string[] = [''];
  private readonly ids = new Map<string, number>([['', 0]]);

  id(name: string): number {
    let i = this.ids.get(name);
    if (i === undefined) {
      i = this.names.length;
      this.names.push(name);
      this.ids.set(name, i);
    }
    return i;
  }

  name(id: number): string {
    return this.names[id] ?? `#${id}`;
  }

  get list(): readonly string[] {
    return this.names;
  }

  load(list: readonly string[]): void {
    this.names = [...list];
    this.ids.clear();
    this.names.forEach((n, i) => this.ids.set(n, i));
  }
}
