import { Link } from 'react-router-dom'
import { Empty } from '../components/ui'

export default function NotFound() {
  return (
    <Empty
      title="Off the map"
      hint="That route doesn't exist."
      action={
        <Link to="/" className="eyebrow border border-rule2 px-3.5 py-2 text-xs text-ink hover:border-accent">
          Back to Deck
        </Link>
      }
    />
  )
}
