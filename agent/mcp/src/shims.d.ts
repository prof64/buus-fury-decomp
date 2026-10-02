interface ImportMeta {
  env: Record<string, string>
  glob: (...a: unknown[]) => Record<string, () => Promise<unknown>>
}
