import { invoke } from "@tauri-apps/api/core";

export type AccountKind = "totp" | "hotp" | "steam";
export type Algorithm = "SHA1" | "SHA256" | "SHA512";

// A type alias rather than an interface: Tauri's `invoke` takes
// `Record<string, unknown>`, and TypeScript grants an implicit index signature
// to type aliases but not to interfaces.
export type PreviewInput = {
  secret: string;
  kind: AccountKind;
  algorithm: Algorithm;
  digits: number;
  period: number;
  counter: number;
};

export interface CodeView {
  code: string;
  secondsRemaining: number;
}

interface RawCodeView {
  code: string;
  seconds_remaining: number;
}

/** Generate the code for the given parameters as of now. */
export async function previewCode(input: PreviewInput): Promise<CodeView> {
  const raw = await invoke<RawCodeView>("preview_code", input);
  return { code: raw.code, secondsRemaining: raw.seconds_remaining };
}
