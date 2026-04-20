// docs/components/landing/rainEngine.test.ts
import { test, expect } from 'bun:test';
import { ageToColor } from './rainEngine';

test('ageToColor returns WHITE for fresh cells (age 0)', () => {
  expect(ageToColor(0)).toBe('rgb(255,255,255)');
});

test('ageToColor returns pale green for age 1-2', () => {
  expect(ageToColor(1)).toBe('rgb(180,255,180)');
  expect(ageToColor(2)).toBe('rgb(180,255,180)');
});

test('ageToColor returns PHOSPHOR_GREEN for age 3-5', () => {
  expect(ageToColor(3)).toBe('rgb(0,255,65)');
  expect(ageToColor(5)).toBe('rgb(0,255,65)');
});

test('ageToColor returns null for dead cells (age > 24)', () => {
  expect(ageToColor(25)).toBeNull();
  expect(ageToColor(100)).toBeNull();
});

import { createRainState, tickRain } from './rainEngine';
import type { RainState } from './rainEngine';

function makeRng(seed: number) {
  return () => {
    // xorshift for deterministic testing
    seed ^= seed << 13; seed ^= seed >>> 17; seed ^= seed << 5;
    return ((seed >>> 0) % 10000) / 10000;
  };
}

test('tickRain advances frame and mutates grid deterministically', () => {
  const rng1 = makeRng(12345);
  const state1 = createRainState(8, 8, rng1);
  tickRain(state1, rng1);
  tickRain(state1, rng1);

  const rng2 = makeRng(12345);
  const state2 = createRainState(8, 8, rng2);
  tickRain(state2, rng2);
  tickRain(state2, rng2);

  expect(state1.frame).toBe(2);
  expect(state2.frame).toBe(2);
  expect(JSON.stringify(state1.grid)).toBe(JSON.stringify(state2.grid));
});
