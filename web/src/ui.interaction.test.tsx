// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Details } from './components/controls';
import { Appearance } from './theme';
import { Modal } from './Modal';
import { ConfirmProvider, useConfirm } from './components/confirm';
afterEach(cleanup);
it('keeps methods collapsed until requested', async()=>{render(<Details><summary>Methods</summary><p>Evidence</p></Details>);const trigger=screen.getByRole('button',{name:'Methods'});expect(trigger.getAttribute('aria-expanded')).toBe('false');await userEvent.click(trigger);expect(trigger.getAttribute('aria-expanded')).toBe('true');});
it('restores the dialog opener on Escape', async()=>{const close=vi.fn();const button=document.createElement('button');document.body.append(button);button.focus();const view=render(<Modal title="Item details" onClose={close}><p>Observed prices</p></Modal>);await userEvent.keyboard('{Escape}');expect(close).toHaveBeenCalledOnce();view.unmount();await waitFor(()=>expect(document.activeElement).toBe(button));button.remove();});
it('cancels destructive actions by default',async()=>{const action=vi.fn();function Example(){const confirm=useConfirm();return <button onClick={async()=>{if(await confirm('Revoke collector?'))action()}}>Revoke</button>};render(<ConfirmProvider><Example/></ConfirmProvider>);await userEvent.click(screen.getByText('Revoke'));await userEvent.click(screen.getByText('Cancel'));expect(action).not.toHaveBeenCalled();});


it('opens a settings section when an asynchronous result requests it', async()=>{const view=render(<Details className="settings-section"><summary>Updates</summary><p>No update yet</p></Details>);expect(screen.getByRole('button',{name:'Updates'}).getAttribute('aria-expanded')).toBe('false');view.rerender(<Details className="settings-section" open><summary>Updates</summary><p>Version available</p></Details>);await waitFor(()=>expect(screen.getByRole('button',{name:'Updates'}).getAttribute('aria-expanded')).toBe('true'));});
it('opens a collapsed settings section when an operational error arrives', async()=>{const view=render(<Details className="settings-section"><summary>Connection</summary><p>Ready</p></Details>);view.rerender(<Details className="settings-section"><summary>Connection</summary><p role="alert">Server offline</p></Details>);await waitFor(()=>expect(screen.getByRole('button',{name:/Connection/}).getAttribute('aria-expanded')).toBe('true'));});
it('restores direct-link dialog focus to the page when no opener exists', async()=>{const main=document.createElement('main');main.id='main-content';main.tabIndex=-1;document.body.append(main);document.body.tabIndex=-1;document.body.focus();const view=render(<Modal title="Direct item" onClose={()=>{}}><p>History</p></Modal>);view.unmount();await waitFor(()=>expect(document.activeElement).toBe(main));main.remove();});
