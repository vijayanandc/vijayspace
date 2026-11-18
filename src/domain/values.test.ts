import { describe, expect, it } from 'vitest';

import {
  MAX_DELAY_SECS, U16_MAX, bestDelayUnit, changeDelayUnit, clampInt, delaySeconds, delayUnit, parseInteger, parseLooseValue, parseNumberField,
} from './values';

describe('integer settings', () => {
  it('rounds and clamps typed integers, and ignores empty or non-numeric text', () => {
    expect(parseInteger('3.6', { min: 1, max: 20 })).toBe(4);
    expect(parseInteger('-5', { min: 1, max: 20 })).toBe(1);
    expect(parseInteger('99', { min: 1, max: 20 })).toBe(20);
    expect(parseInteger('0', { min: 0, max: U16_MAX })).toBe(0);
    expect(parseInteger('', { min: 1, max: 20 })).toBeUndefined();
    expect(parseInteger('abc', { min: 1, max: 20 })).toBeUndefined();
    expect(clampInt(1e30, { min: 60, max: 4_294_967_295 })).toBe(4_294_967_295);
  });
});

describe('delay units', () => {
  it('picks the largest unit that shows a whole amount', () => {
    expect(bestDelayUnit(86_400 * 2)).toBe(86_400);
    expect(bestDelayUnit(7200)).toBe(3600);
    expect(bestDelayUnit(1860)).toBe(60);
    expect(bestDelayUnit(90)).toBe(1);
  });

  it('keeps the chosen unit while typing (120 minutes does not become 2 hours)', () => {
    expect(delayUnit(7200, 60)).toBe(60);
    expect(delayUnit(7200, null)).toBe(3600);
    expect(delayUnit(90, 60)).toBe(1);
  });

  it('always produces a whole, positive number of seconds within the server maximum', () => {
    expect(delaySeconds(0, 60)).toBe(60);
    expect(delaySeconds(1.4, 3600)).toBe(3600);
    expect(delaySeconds(15, 86_400)).toBe(MAX_DELAY_SECS);
    expect(MAX_DELAY_SECS).toBe(14 * 86_400);
    expect(changeDelayUnit(1860, 60, 3600)).toBe(31 * 3600);
    expect(changeDelayUnit(90, 1, 60)).toBe(5400);
    expect(Number.isInteger(changeDelayUnit(1, 60, 3600))).toBe(true);
  });
});

describe('mappable number fields', () => {
  it('stores complete numbers, keeps templates and text, and waits on partial numbers', () => {
    expect(parseNumberField('1.5')).toBe(1.5);
    expect(parseNumberField('-2e3')).toBe(-2000);
    expect(parseNumberField('1.')).toBeUndefined();
    expect(parseNumberField('-')).toBeUndefined();
    expect(parseNumberField('2e')).toBeUndefined();
    expect(parseNumberField('{{ trigger.qty }}')).toBe('{{ trigger.qty }}');
    expect(parseNumberField('')).toBe('');
    expect(parseNumberField('ten')).toBe('ten');
  });
});

describe('loose values', () => {
  it('parses JSON other than strings and keeps everything else as text', () => {
    expect(parseLooseValue('[1,2]')).toEqual([1, 2]);
    expect(parseLooseValue('{"a": "{{ x }}"}')).toEqual({ a: '{{ x }}' });
    expect(parseLooseValue('true')).toBe(true);
    expect(parseLooseValue('null')).toBeNull();
    expect(parseLooseValue('42')).toBe(42);
    expect(parseLooseValue('4876876000000123456')).toBe('4876876000000123456');
    expect(parseLooseValue('1.50')).toBe('1.50');
    expect(parseLooseValue('[1,')).toBe('[1,');
    expect(parseLooseValue('{{ trigger.name }}')).toBe('{{ trigger.name }}');
    expect(parseLooseValue('"quoted"')).toBe('"quoted"');
  });
});
