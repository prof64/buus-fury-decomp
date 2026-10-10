import { Log1Formulas } from './formulas/Log1Formulas'
import { Log2Formulas } from './formulas/Log2Formulas'
import { BuuFormulas } from './formulas/BuuFormulas'

export function FormulasPage({ game }: { game: string }) {
  if (game === 'log1') return <Log1Formulas />
  if (game === 'log2') return <Log2Formulas />
  return <BuuFormulas />
}
