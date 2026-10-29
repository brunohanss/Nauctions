import { invoke, isTauri } from '@tauri-apps/api/core';
export const isDesktop = () => typeof window !== 'undefined' && isTauri();
export const desktopInvoke = invoke;
export type DesktopConfig = { server: string | null; game_root: string | null; game_exe: string | null; launch_at_login: boolean; automatic_collection: boolean };
export type CollectorStatus = { running: boolean; paired: boolean; last_exit: string | null; game_state: 'running' | 'closed' | 'unknown'; capture_mode: 'playing' | 'final_save' | 'standby' | 'detection_unknown' | null; startup_enabled: boolean | null; startup_error: string | null };
export const currentRoute = () => isDesktop() ? location.hash.slice(1).split('?')[0] || '/opportunities' : location.pathname;
export const routeUrl = (path: string) => isDesktop() ? `#${path}` : path;
