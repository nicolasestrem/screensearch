import { useEffect, useState } from 'react'
import clsx from 'clsx'
import { getFrameImage } from '../lib/api'

/**
 * Loads a frame screenshot as an object URL. Object URLs are cached and shared
 * across mounts by the api layer (a bounded LRU that revokes evicted URLs), so we
 * intentionally do not revoke on unmount (the same id is re-requested often as the
 * user scrubs/scrolls).
 */
export function FrameImage({
  id,
  alt,
  className,
}: {
  id: number
  alt: string
  className?: string
}) {
  const [url, setUrl] = useState<string | null>(null)
  const [failed, setFailed] = useState(false)

  useEffect(() => {
    let alive = true
    setUrl(null)
    setFailed(false)
    getFrameImage(id)
      .then((u) => {
        if (alive) setUrl(u)
      })
      .catch(() => {
        if (alive) setFailed(true)
      })
    return () => {
      alive = false
    }
  }, [id])

  if (failed) {
    return (
      <div className={clsx('flex items-center justify-center bg-void font-mono text-xs text-faint', className)}>
        no image
      </div>
    )
  }
  if (!url) {
    return <div className={clsx('animate-pulse bg-panel2', className)} />
  }
  return <img src={url} alt={alt} loading="lazy" className={className} />
}
