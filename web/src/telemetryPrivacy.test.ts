import { describe, expect, it } from 'vitest';
import { safeExceptionList, safeOperation, safeProperties, safeScreen } from './telemetryPrivacy';

describe('telemetry privacy boundary', () => {
  it('excludes search text, credentials, payloads and local paths', () => {
    expect(safeProperties({ token: 'secret', body: { password: 'secret' }, q: 'private search', operation: 'C:\\Users\\Alice', status: 503, request_id: 'abc-123' })).toEqual({ status: 503, request_id: 'abc-123' });
  });
  it('replaces item and member IDs and drops query strings', () => {
    expect(safeOperation('/recipes/123/quote?nickname=Alice')).toBe('recipes/id/quote');
    expect(safeOperation('/admin/members/private-name')).toBe('admin/members/id');
    expect(safeScreen('/items/private-item')).toBe('item_detail');
  });
  it('removes exception messages, URL queries and absolute local paths', () => {
    const text = JSON.stringify(safeExceptionList([{ value: 'password=secret', stacktrace: { frames: [{ filename: 'https://example.test/assets/app.js?token=secret', function: 'render', lineno: 10 }, { filename: 'C:\\Users\\Alice\\app.js' }] } }]));
    expect(text).not.toContain('secret'); expect(text).not.toContain('Alice');
    expect(text).toContain('/assets/app.js'); expect(text).toContain('render');
  });
});
