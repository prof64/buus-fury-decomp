// Image loading with a shared cache. Paths in the data files are relative to the site's public root
// (e.g. "assets/log2/sprites/103_piccolo/sheet.png").

const cache = new Map<string, Promise<HTMLImageElement>>()
const ready = new Map<string, HTMLImageElement>()

export function assetUrl(path: string): string {
  if (/^(https?:|data:|\/)/.test(path)) return path
  return import.meta.env.BASE_URL + path
}

export function loadImage(path: string): Promise<HTMLImageElement> {
  let p = cache.get(path)
  if (!p) {
    p = new Promise((resolve, reject) => {
      const img = new Image()
      img.onload = () => {
        ready.set(path, img)
        resolve(img)
      }
      img.onerror = () => reject(new Error('failed to load ' + path))
      img.src = assetUrl(path)
    })
    cache.set(path, p)
  }
  return p
}

/** Synchronous access to an image that already finished loading (null otherwise; kicks off loading). */
export function imageNow(path: string): HTMLImageElement | null {
  const r = ready.get(path)
  if (r) return r
  loadImage(path).catch(() => undefined)
  return null
}

export async function loadAll(paths: Iterable<string>): Promise<void> {
  await Promise.all([...new Set(paths)].map((p) => loadImage(p).catch(() => null)))
}
