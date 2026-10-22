// Where the character icons are: one PNG per character id, in the app's own
// files (`public/characters/`), so nothing is fetched from the internet.

export function characterIconUrl(id: number): string {
  return `/characters/${id}.png`
}
