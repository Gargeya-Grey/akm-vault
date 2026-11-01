import { invalidateSession, sessionGeneration } from "./sessionBoundary";
import type { Status } from "./types";

export const NATIVE_OPEN_EVENT = "akm-native-opening";
export const NATIVE_INVALIDATION_EVENT = "akm-native-invalidated";
export const PROTECTED_VIEW_EXPIRY_EVENT = "akm-protected-view-expired";
export const NATIVE_PROTOCOL_ERROR = "Native session ownership could not be validated. Could not confirm the native lock. Close AKM before leaving this computer.";
const CANCEL_MESSAGE = "The native session changed. Unlock the vault again.";

let owner: string | null = null;
let nextOpening = 0;
let opening: { id: number; generation: number } | null = null;
let noticeStamp: object = {};
// Opaque native IDs only. Includes invalidations delivered before a reservation/status reply.
// This document-lifetime cache survives React roots, not a native process restart or webview reload.
const invalidated = new Set<string>();
// Opaque native notice identities only. Unknown-owner hides are coalesced until
// adoption; a duplicate cannot erase a form opened after the first delivery.
const viewNotices = new Map<string, Set<string>>();
const pendingViewExpiry = new Set<string>();

function adoptedViewExpiry(id: string) {
  if (pendingViewExpiry.delete(id)) window.dispatchEvent(new CustomEvent(PROTECTED_VIEW_EXPIRY_EVENT));
}

function ownerId(value: unknown): string | null {
  return typeof value === "string" && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value) ? value : null;
}

export function nativeOwner() { return owner; }
export function nativeNoticeStamp() { return noticeStamp; }
export function nativeOpening() { return opening?.generation === sessionGeneration(); }
export function nativeCancellation() { return { kind: "locked", message: CANCEL_MESSAGE }; }

export function invalidateNativeView(message?: string) {
  invalidateSession();
  window.dispatchEvent(new CustomEvent(NATIVE_INVALIDATION_EVENT, { detail: { message } }));
}

export function beginNativeOpen() {
  invalidateSession();
  owner = null; // Do not let a prior owner's queued event target the new attempt.
  const token = { id: ++nextOpening, generation: sessionGeneration() };
  opening = token;
  window.dispatchEvent(new CustomEvent(NATIVE_OPEN_EVENT));
  return token;
}

export function endNativeOpen(token: { id: number; generation: number }) {
  if (opening?.id === token.id) opening = null;
}

export function releaseNativeOwner(expected: string) {
  if (owner === expected) owner = null;
}

export function adoptReservation(value: unknown): string {
  const id = ownerId(value);
  if (id === null) {
    invalidateNativeView(NATIVE_PROTOCOL_ERROR);
    throw nativeCancellation();
  }
  if (invalidated.has(id)) {
    invalidateNativeView();
    throw nativeCancellation();
  }
  owner = id;
  adoptedViewExpiry(id);
  return id;
}

export function confirmNativeLock(expected: string | null) {
  if (expected !== null) invalidated.add(expected);
  if (owner !== null && owner === expected) {
    owner = null;
    invalidateNativeView();
  }
}

export function receiveNativeLock(payload: unknown): boolean {
  const id = ownerId(payload && typeof payload === "object" && "sessionId" in payload ? payload.sessionId : undefined);
  if (id === null) return false; // App explicitly fails closed and attempts an owned native lock.
  noticeStamp = {};
  confirmNativeLock(id);
  return true;
}

export function receiveNativeViewExpiry(payload: unknown): boolean {
  const data = payload && typeof payload === "object" ? payload : null;
  const id = ownerId(data && "sessionId" in data ? data.sessionId : undefined);
  const hide = ownerId(data && "hideId" in data ? data.hideId : undefined);
  if (id === null || hide === null) return false;
  let seen = viewNotices.get(id);
  if (!seen) { seen = new Set(); viewNotices.set(id, seen); }
  if (seen.has(hide)) return true;
  seen.add(hide);
  if (owner === id) window.dispatchEvent(new CustomEvent(PROTECTED_VIEW_EXPIRY_EVENT));
  else pendingViewExpiry.add(id);
  return true;
}

export function acceptNativeStatus(value: Status, expected?: string) {
  if (!value || typeof value.unlocked !== "boolean" || !("sessionId" in value)) {
    invalidateNativeView(NATIVE_PROTOCOL_ERROR);
    throw nativeCancellation();
  }
  const id = ownerId(value.sessionId);
  if (value.unlocked) {
    if (id === null || (expected !== undefined && id !== expected)) {
      invalidateNativeView(NATIVE_PROTOCOL_ERROR);
      throw nativeCancellation();
    }
    if (invalidated.has(id)) {
      invalidateNativeView();
      throw nativeCancellation();
    }
    owner = id;
    adoptedViewExpiry(id);
  } else {
    if (value.sessionId !== null) {
      invalidateNativeView(NATIVE_PROTOCOL_ERROR);
      throw nativeCancellation();
    }
    if (!nativeOpening()) confirmNativeLock(owner);
  }
}
