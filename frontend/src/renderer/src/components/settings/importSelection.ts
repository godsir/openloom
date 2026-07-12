export function toggleId(selected: string[], id: string): string[] {
  return selected.includes(id) ? selected.filter((x) => x !== id) : [...selected, id]
}

export function allSelected(ids: string[], selected: string[]): boolean {
  return ids.length > 0 && ids.every((id) => selected.includes(id))
}
