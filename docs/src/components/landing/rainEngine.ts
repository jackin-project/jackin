// docs/components/landing/rainEngine.ts

// Exact char pool from src/tui.rs line 76-77
export const RAIN_CHARS = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz@#$%&*<>{}[]|/\\~';

export type RainTheme = 'dark' | 'light';

// Dark mode: phosphor-green rain on near-black hero — exact palette from
// src/tui.rs lines 13-19 + age_to_color (lines 55-64).
// Light mode: black rain on the green hero — the visual inverse. Leader
// goes pure black for max pop against the saturated green; older cells
// fade toward transparency so the tail bleeds into the bg instead of
// tracking as gray noise.
export function ageToColor(age: number, theme: RainTheme = 'dark'): string | null {
  if (theme === 'light') {
    if (age === 0)  return 'rgb(0,0,0)';           // BLACK — leader
    if (age <= 2)   return 'rgba(0,0,0,0.8)';
    if (age <= 5)   return 'rgba(0,0,0,0.6)';
    if (age <= 10)  return 'rgba(0,0,0,0.4)';
    if (age <= 16)  return 'rgba(0,0,0,0.22)';
    if (age <= 24)  return 'rgba(0,0,0,0.1)';
    return null;
  }
  if (age === 0)  return 'rgb(255,255,255)';   // WHITE — leader
  if (age <= 2)   return 'rgb(180,255,180)';   // pale green
  if (age <= 5)   return 'rgb(0,255,65)';      // PHOSPHOR_GREEN
  if (age <= 10)  return 'rgb(0,200,50)';      // mid green
  if (age <= 16)  return 'rgb(0,140,30)';      // PHOSPHOR_DIM
  if (age <= 24)  return 'rgb(0,80,18)';       // PHOSPHOR_DARK
  return null;
}

// Mutation probability from src/tui.rs lines 67-74
export function shouldMutate(age: number, rng: () => number = Math.random): boolean {
  const roll = rng() * 100;
  if (age <= 2)  return roll < 30;
  if (age <= 10) return roll < 15;
  return roll < 5;
}

export function randomChar(rng: () => number = Math.random): string {
  return RAIN_CHARS[Math.floor(rng() * RAIN_CHARS.length)];
}

export interface RainCell {
  ch: string;
  age: number;
  fade: number;
}

export interface RainColumn {
  head: number;
  speed: number;
  fade: number;
  active: boolean;
  cooldown: number;
}

export interface RainState {
  cols: number;
  rows: number;
  grid: (RainCell | null)[][];
  columns: RainColumn[];
  frame: number;
}

export function createRainState(cols: number, rows: number, rng: () => number = Math.random): RainState {
  const columns: RainColumn[] = Array.from({ length: cols }, () => ({
    head: -Math.floor(rng() * (rows + 6)),
    speed: 1 + Math.floor(rng() * 4),
    fade: 1 + Math.floor(rng() * 3),
    active: rng() >= 0.33,
    cooldown: 0,
  }));
  const grid: (RainCell | null)[][] = Array.from({ length: rows }, () => new Array(cols).fill(null));
  return { cols, rows, columns, grid, frame: 0 };
}

export function tickRain(state: RainState, rng: () => number = Math.random): void {
  // Age all cells
  for (let r = 0; r < state.rows; r++) {
    const row = state.grid[r];
    for (let c = 0; c < state.cols; c++) {
      const cell = row[c];
      if (!cell) continue;
      cell.age += cell.fade;
      if (ageToColor(cell.age) === null) {
        row[c] = null;
      } else if (shouldMutate(cell.age, rng)) {
        cell.ch = randomChar(rng);
      }
    }
  }

  // Advance columns
  for (let c = 0; c < state.cols; c++) {
    const col = state.columns[c];
    if (!col.active) {
      if (col.cooldown > 0) col.cooldown--;
      else {
        col.active = true;
        col.head = -Math.floor(rng() * 6);
        col.speed = 1 + Math.floor(rng() * 4);
        col.fade = 1 + Math.floor(rng() * 3);
      }
      continue;
    }
    if (state.frame % col.speed === 0) col.head++;
    if (col.head >= 0 && col.head < state.rows) {
      state.grid[col.head][c] = { ch: randomChar(rng), age: 0, fade: col.fade };
    }
    if (col.head > state.rows + 5) {
      col.active = false;
      col.cooldown = 2 + Math.floor(rng() * 18);
    }
  }

  state.frame++;
}
