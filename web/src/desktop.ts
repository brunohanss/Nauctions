import { invoke, isTauri } from '@tauri-apps/api/core';
import { reportFailure, track } from './telemetry';
export const isDesktop = () => typeof window !== 'undefined' && isTauri();
export async function desktopInvoke<T>(command: string, args?: Parameters<typeof invoke>[1]): Promise<T> {
  try {
    const result = await invoke<T>(command, args);
    if (['configure_server', 'select_game_path', 'set_desktop_preferences', 'install_update', 'collector_action'].includes(command)) {
      const operation = args && 'action' in args ? args.action : undefined;
      track('desktop_action_completed', { action: command, ...(command === 'collector_action' && typeof operation === 'string' ? { operation } : {}) });
    }
    return result;
  } catch (error) {
    reportFailure('desktop_command_failed', { operation: command }, error);
    throw error;
  }
}
export type DesktopConfig = { server: string | null; game_root: string | null; game_exe: string | null; launch_at_login: boolean; automatic_collection: boolean };
export type CollectorStatus = { health?: { authentication_paused: boolean; acquisition_errors: number; rejected_count: number; queue_count: number; oldest_pending_seconds?: number | null; source_unavailable?: boolean } | null; running: boolean; paired: boolean; last_exit: string | null; game_state: 'running' | 'closed' | 'unknown'; capture_mode: 'playing' | 'final_save' | 'standby' | 'detection_unknown' | null; startup_enabled: boolean | null; startup_error: string | null };
export const currentRoute = () => isDesktop() ? location.hash.slice(1).split('?')[0] || '/opportunities' : location.pathname;
export const routeUrl = (path: string) => isDesktop() ? `#${path}` : path;
