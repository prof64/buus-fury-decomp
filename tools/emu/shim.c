// Thin C shim over libmgba so Python (ctypes) can drive a GBA core headlessly:
// run frames, set keys, read/write memory, screenshots, save states, and log
// watchpoint / breakpoint hits (with CPU registers) without pausing.
#include <mgba/core/core.h>
#include <mgba/core/config.h>
#include <mgba/core/log.h>
#include <mgba/core/serialize.h>
#include <mgba/debugger/debugger.h>
#include <mgba/internal/arm/arm.h>
#include <mgba/internal/arm/debugger/debugger.h>
#include <mgba-util/vfs.h>
#include <stdlib.h>
#include <string.h>

#define MAX_HITS 65536

struct Hit {
	uint32_t kind;  // 1 = watchpoint, 2 = breakpoint
	uint32_t address;
	uint32_t oldValue;
	uint32_t newValue;
	uint32_t accessType;
	uint32_t width;
	uint32_t frame;
	int32_t regs[16];
	uint32_t cpsr;
};

struct Emu {
	struct mCore* core;
	uint32_t* video;
	struct mDebugger debugger;
	struct mDebuggerModule module;
	bool hasDebugger;
	struct Hit hits[MAX_HITS];
	uint32_t nHits;
	uint32_t dropped;
};

static void _entered(struct mDebuggerModule* module, enum mDebuggerEntryReason reason, struct mDebuggerEntryInfo* info) {
	struct Emu* emu = (struct Emu*) ((char*) module - offsetof(struct Emu, module));
	module->isPaused = false;
	if (!info || (reason != DEBUGGER_ENTER_WATCHPOINT && reason != DEBUGGER_ENTER_BREAKPOINT)) {
		return;
	}
	if (emu->nHits >= MAX_HITS) {
		++emu->dropped;
		return;
	}
	struct Hit* h = &emu->hits[emu->nHits++];
	memset(h, 0, sizeof(*h));
	h->kind = reason == DEBUGGER_ENTER_WATCHPOINT ? 1 : 2;
	h->address = info->address;
	h->width = info->width;
	if (reason == DEBUGGER_ENTER_WATCHPOINT) {
		h->oldValue = info->type.wp.oldValue;
		h->newValue = info->type.wp.newValue;
		h->accessType = info->type.wp.accessType;
	}
	h->frame = emu->core->frameCounter(emu->core);
	struct ARMCore* cpu = emu->core->cpu;
	memcpy(h->regs, cpu->gprs, sizeof(h->regs));
	h->cpsr = cpu->cpsr.packed;
}

static void _nolog(struct mLogger* l, int c, enum mLogLevel lv, const char* f, va_list a) {
	(void) l; (void) c; (void) lv; (void) f; (void) a;
}
static struct mLogger _quiet = { .log = _nolog };

struct Emu* emu_open(const char* path) {
	mLogSetDefaultLogger(&_quiet);
	struct mCore* core = mCoreFind(path);
	if (!core) return NULL;
	core->init(core);
	struct Emu* emu = calloc(1, sizeof(*emu));
	emu->core = core;
	unsigned w, h;
	core->baseVideoSize(core, &w, &h);
	emu->video = calloc(w * h, 4);
	core->setVideoBuffer(core, (mColor*) emu->video, w);
	mCoreInitConfig(core, NULL);
	struct mCoreOptions opts = { .useBios = false, .skipBios = true, .volume = 0, .audioBuffers = 1024 };
	mCoreConfigLoadDefaults(&core->config, &opts);
	mCoreLoadConfig(core);
	if (!mCoreLoadFile(core, path)) return NULL;
	core->reset(core);
	return emu;
}

void emu_enable_debugger(struct Emu* emu) {
	if (emu->hasDebugger) return;
	mDebuggerInit(&emu->debugger);
	memset(&emu->module, 0, sizeof(emu->module));
	emu->module.type = DEBUGGER_CUSTOM;
	emu->module.entered = _entered;
	mDebuggerAttach(&emu->debugger, emu->core);
	mDebuggerAttachModule(&emu->debugger, &emu->module);
	emu->hasDebugger = true;
}

int emu_watch(struct Emu* emu, uint32_t start, uint32_t end, int type) {
	emu_enable_debugger(emu);
	struct mWatchpoint wp = { .minAddress = start, .maxAddress = end, .segment = -1, .type = type };
	return (int) emu->debugger.platform->setWatchpoint(emu->debugger.platform, &emu->module, &wp);
}

int emu_break(struct Emu* emu, uint32_t address) {
	emu_enable_debugger(emu);
	struct mBreakpoint bp = { .address = address, .segment = -1, .type = BREAKPOINT_HARDWARE };
	return (int) emu->debugger.platform->setBreakpoint(emu->debugger.platform, &emu->module, &bp);
}

int emu_clear(struct Emu* emu, int id) {
	if (!emu->hasDebugger) return 0;
	return emu->debugger.platform->clearBreakpoint(emu->debugger.platform, id);
}

uint32_t emu_hits(struct Emu* emu, struct Hit** out, uint32_t* dropped) {
	*out = emu->hits;
	*dropped = emu->dropped;
	uint32_t n = emu->nHits;
	return n;
}

void emu_clear_hits(struct Emu* emu) {
	emu->nHits = 0;
	emu->dropped = 0;
}

void emu_run_frames(struct Emu* emu, int n) {
	for (int i = 0; i < n; ++i) {
		if (emu->hasDebugger) {
			mDebuggerRunFrame(&emu->debugger);
		} else {
			emu->core->runFrame(emu->core);
		}
	}
}

void emu_step(struct Emu* emu) { emu->core->step(emu->core); }
void emu_set_keys(struct Emu* emu, uint32_t keys) { emu->core->setKeys(emu->core, keys); }
uint32_t emu_frame(struct Emu* emu) { return emu->core->frameCounter(emu->core); }

uint32_t emu_read8(struct Emu* emu, uint32_t a) { return emu->core->rawRead8(emu->core, a, -1); }
uint32_t emu_read16(struct Emu* emu, uint32_t a) { return emu->core->rawRead16(emu->core, a, -1); }
uint32_t emu_read32(struct Emu* emu, uint32_t a) { return emu->core->rawRead32(emu->core, a, -1); }
void emu_write8(struct Emu* emu, uint32_t a, uint8_t v) { emu->core->rawWrite8(emu->core, a, -1, v); }
void emu_write16(struct Emu* emu, uint32_t a, uint16_t v) { emu->core->rawWrite16(emu->core, a, -1, v); }
void emu_write32(struct Emu* emu, uint32_t a, uint32_t v) { emu->core->rawWrite32(emu->core, a, -1, v); }

void emu_read_block(struct Emu* emu, uint32_t a, uint8_t* out, uint32_t n) {
	for (uint32_t i = 0; i < n; ++i) out[i] = emu->core->rawRead8(emu->core, a + i, -1);
}

int32_t emu_reg(struct Emu* emu, int i) {
	struct ARMCore* cpu = emu->core->cpu;
	if (i == 16) return cpu->cpsr.packed;
	return cpu->gprs[i];
}

void emu_set_reg(struct Emu* emu, int i, int32_t v) {
	struct ARMCore* cpu = emu->core->cpu;
	cpu->gprs[i] = v;
}

const uint32_t* emu_video(struct Emu* emu) { return emu->video; }

size_t emu_state_size(struct Emu* emu) { return emu->core->stateSize(emu->core); }
int emu_save_state(struct Emu* emu, void* buf) { return emu->core->saveState(emu->core, buf); }
int emu_load_state(struct Emu* emu, const void* buf) { return emu->core->loadState(emu->core, buf); }

void emu_close(struct Emu* emu) {
	emu->core->deinit(emu->core);
	free(emu->video);
	free(emu);
}
