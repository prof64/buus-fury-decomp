import { BTN } from '../../src/battle/core/input'

/** emulator key names (tools/emu KEY) -> GBA button bits (same KEYINPUT order) */
export const KEY_BITS: Record<string, number> = { ...BTN }
