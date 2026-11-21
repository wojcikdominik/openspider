// Generiert von scripts/gen-ipc.mjs aus src/ipc/commands.json – nicht von Hand ändern (npm run gen:ipc).
import type { UpdateChannelState } from "./gen/UpdateChannelState";
import type { UpdateInstallArgs } from "./gen/UpdateInstallArgs";
import type { UpdateSetAutoArgs } from "./gen/UpdateSetAutoArgs";
import type { UpdateSetChannelArgs } from "./gen/UpdateSetChannelArgs";
import type { UpdateState } from "./gen/UpdateState";
import { client } from "./client";

export const update = {
  /** `update.check` */
  check: (signal?: AbortSignal) => client.invoke<UpdateState>("update.check", undefined, signal),
  /** `update.status` */
  status: (signal?: AbortSignal) => client.invoke<UpdateState>("update.status", undefined, signal),
  /** `update.download` */
  download: (signal?: AbortSignal) => client.invoke<UpdateState>("update.download", undefined, signal),
  /** `update.install` */
  install: (args: UpdateInstallArgs, signal?: AbortSignal) => client.invoke<UpdateState>("update.install", args, signal),
  /** `update.set_auto` */
  set_auto: (args: UpdateSetAutoArgs, signal?: AbortSignal) => client.invoke<void>("update.set_auto", args, signal),
  /** `update.channel` */
  channel: (signal?: AbortSignal) => client.invoke<UpdateChannelState>("update.channel", undefined, signal),
  /** `update.set_channel` */
  set_channel: (args: UpdateSetChannelArgs, signal?: AbortSignal) => client.invoke<UpdateChannelState>("update.set_channel", args, signal),
} as const;
