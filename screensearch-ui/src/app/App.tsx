import { BrowserRouter, Route, Routes } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { AppShell } from './shell/AppShell'
import Deck from '../pages/Deck'
import Recall from '../pages/Recall'
import Timeline from '../pages/Timeline'
import Moment from '../pages/Moment'
import Insights from '../pages/Insights'
import Settings from '../pages/Settings'
import NotFound from '../pages/NotFound'

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      retry: 1,
      staleTime: 30000,
    },
  },
})

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <Routes>
          <Route element={<AppShell />}>
            <Route path="/" element={<Deck />} />
            <Route path="/recall" element={<Recall />} />
            <Route path="/timeline" element={<Timeline />} />
            <Route path="/timeline/:id" element={<Moment />} />
            <Route path="/insights" element={<Insights />} />
            <Route path="/settings" element={<Settings />} />
            <Route path="*" element={<NotFound />} />
          </Route>
        </Routes>
      </BrowserRouter>
    </QueryClientProvider>
  )
}
