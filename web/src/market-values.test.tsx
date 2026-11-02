import { describe, expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { WowMoney } from './WowMoney';
import { Percentage } from './Percentage';
import { moneyParts, moneyText } from './money';

const amount = (numerator: string, denominator = '1') => ({ numerator, denominator });

describe('whole copper display', () => {
  it.each([
    ['21170885', '4', '529g 27s 21c'],
    ['1', '2', '1c'],
    ['1', '3', '0c'],
    ['199', '2', '1s 0c'],
    ['19999', '2', '1g 0c'],
    ['-19999', '2', '−1g 0c'],
    ['-1', '3', '0c'],
    ['9007199254740993', '1', '900,719,925,474g 9s 93c'],
  ])('formats %s/%s as %s', (n, d, expected) => {
    expect(moneyText(amount(n, d))).toBe(expected);
  });

  it('renders each denomination with an icon and an accessible text equivalent', () => {
    const html = renderToStaticMarkup(<WowMoney value={amount('5292721')} />);
    expect(html).toContain('aria-label="529g 27s 21c"');
    for (const coin of ['gold', 'silver', 'copper']) expect(html).toContain(`coin-${coin}`);
    expect(html).not.toContain('21.25');
  });

  it('handles signed profit, zero, and missing or invalid amounts', () => {
    expect(renderToStaticMarkup(<WowMoney value={amount('-280')} />)).toContain('aria-label="−2s 80c"');
    expect(renderToStaticMarkup(<WowMoney value={amount('0')} />)).toContain('coin-copper');
    for (const value of [null, amount('bad'), amount('1', '0'), amount('1', '-1'), { numerator: 9007199254740992, denominator: 1 }]) {
      expect(moneyParts(value)).toBeNull();
      expect(renderToStaticMarkup(<WowMoney value={value} />)).not.toContain('coin-');
    }
  });
});

describe('percentage colors', () => {
  it.each([[101, 'positive'], [99, 'negative'], [100, 'neutral'], [0, 'neutral']])('compares reference ratio %s with 100', (value, tone) => {
    expect(renderToStaticMarkup(<Percentage value={value} />)).toContain(`percentage-${tone}`);
  });
  it.each([[1, 'positive'], [-1, 'negative'], [0, 'neutral'], [-0.01, 'neutral']])('compares change %s with zero', (value, tone) => {
    expect(renderToStaticMarkup(<Percentage value={value} baseline={0} signed />)).toContain(`percentage-${tone}`);
  });
  it('accepts rational margins and marks lower prices as favorable', () => {
    expect(renderToStaticMarkup(<Percentage value={amount('1', '2')} baseline={0} />)).toContain('0.5%');
    expect(renderToStaticMarkup(<Percentage value={-10} baseline={0} higherIsBetter={false} />)).toContain('percentage-positive');
  });
  it('keeps missing values separate from zero', () => {
    for (const value of [null, NaN, Infinity, amount('1', '0')]) {
      expect(renderToStaticMarkup(<Percentage value={value} />)).not.toContain('percentage-');
    }
  });
});
