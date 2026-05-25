declare module '../../../../shared/studio-access-contract.js' {
  export const STUDIO_ACCESS_CAPABILITIES: readonly string[];
  export function deriveStudioAccessGrant(...args: unknown[]): unknown;
  export function getStudioConnectionProfile(...args: unknown[]): unknown;
  export function validateStudioConnectionTrust(...args: unknown[]): unknown;

  export type ConnectionCredentialKind = string;
  export type OfficialServiceKind = string;
  export type ServerTrustState = string;
  export type StudioAccessActorKind = string;
  export type StudioAccessCapability = string;
  export type StudioAccessDataOwner = string;
  export type StudioAccessGrant = unknown;
  export type StudioAccessConnection = unknown;
  export type StudioConnectionKind = string;
  export type StudioConnectionProfile = unknown;
  export type StudioConnectionTransport = string;
}
