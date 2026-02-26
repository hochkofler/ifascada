# IFASCADA Web UI (Next.js)

## Stack
1. Next.js + TypeScript
2. TanStack Query
3. Zustand
4. ECharts

## Run
```powershell
cd web-ui
npm install
npm run dev
```

Default URL:
- `http://127.0.0.1:3001`

## Backend
Set API base in `.env.local` (copy from `.env.example`):
- `NEXT_PUBLIC_API_BASE=http://127.0.0.1:8088`
- `NEXT_PUBLIC_SSE_URL=http://127.0.0.1:8088/api/stream/events`
