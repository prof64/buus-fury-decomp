// The savedata accessors (the cartridge's EEPROM: in-game saves). build_sb.sh compiles tools/emu/shim.c with this
// file appended, so the save-capable library always has the current shim's functions (a stale full copy lacked
// emu_cycles and broke the cut makers - resync.py / cut.py).
#include <mgba/internal/gba/savedata.h>
int emu_savedata_type(struct Emu* emu) { struct GBA* gba = emu->core->board; return gba->memory.savedata.type; }
size_t emu_savedata_size(struct Emu* emu) { struct GBA* gba = emu->core->board; return GBASavedataSize(&gba->memory.savedata); }
size_t emu_savedata_read(struct Emu* emu, uint8_t* out, size_t n) {
	struct GBA* gba = emu->core->board; struct GBASavedata* s = &gba->memory.savedata;
	size_t sz = GBASavedataSize(s); if (!s->data) return 0; if (n > sz) n = sz; memcpy(out, s->data, n); return n;
}
size_t emu_savedata_write(struct Emu* emu, const uint8_t* in, size_t n) {
	struct GBA* gba = emu->core->board; struct GBASavedata* s = &gba->memory.savedata;
	size_t sz = GBASavedataSize(s); if (!s->data) return 0; if (n > sz) n = sz; memcpy(s->data, in, n); return n;
}
