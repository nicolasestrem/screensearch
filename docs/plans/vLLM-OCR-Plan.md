# ScreenSearch v2: Vision LLM OCR & Intelligence-First Redesign

**Date**: 2024-12-14
**Status**: Design Complete - Pending Implementation
**Author**: Brainstorming Session with Claude

---

## Executive Summary

This document outlines a comprehensive redesign of ScreenSearch to replace the current broken Search/Timeline features and Windows OCR with an intelligence-first approach using Vision LLMs for screen understanding and natural language queries for retrieval.

### Core Problem Statement

The current ScreenSearch implementation has several critical issues:

1. **Windows OCR captures too much noise** - Menus, toolbars, title bars, and static UI elements account for ~80-90% of stored text, drowning out meaningful content
2. **Search and Timeline features are broken** - These core features never worked properly
3. **No visual understanding** - Windows OCR extracts text only; cannot describe images, colors, layouts, or visual context
4. **Real-time processing causes resource spikes** - Users have no control over when heavy processing occurs

### Solution Overview

Replace Windows OCR with a Vision LLM (DeepSeek-VL 1.3B) that provides:
- Intelligent text extraction with context awareness
- Visual description capabilities (colors, layouts, images)
- User-triggered processing for resource control
- Natural language queries with Rich Report Card responses

---

## Table of Contents

1. [Key Decisions](#key-decisions)
2. [Architecture](#architecture)
3. [Vision Model Research & Selection](#vision-model-research--selection)
4. [Dashboard UI Design](#dashboard-ui-design)
5. [Query System Design](#query-system-design)
6. [Rich Report Card Format](#rich-report-card-format)
7. [API Design](#api-design)
8. [Database Schema Changes](#database-schema-changes)
9. [Implementation Phases](#implementation-phases)
10. [File-by-File Implementation Guide](#file-by-file-implementation-guide)
11. [Testing Strategy](#testing-strategy)
12. [Open Questions](#open-questions)
13. [References & Research](#references--research)

---

## Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Primary Focus** | Better Retrieval (not capture changes) | Deliver value faster with existing capture infrastructure |
| **Vision OCR Model** | DeepSeek-VL (1.3B) primary | Smallest footprint, fastest inference, excellent reasoning |
| **Alternative Vision Models** | Moondream2 (1.8B), Qwen2.5-VL-3B | To test later for quality comparison |
| **Query Text LLM** | Ministral-3B (user configurable) | Small, fast, local-first, provider-agnostic |
| **Processing Model** | User-triggered OCR + Indexing | Control over resource usage; no background spikes |
| **Query Modes** | Light (fast) + Heavy (thorough) | Balance speed vs depth based on user needs |
| **Response Format** | Rich Report Cards | Structured summaries with screenshots, timelines, context |
| **UI Paradigm** | Full Dashboard with controls | Replace broken Timeline/Search with unified interface |
| **Query Input** | Example queries (editable) + free-form | Scaffolding for users + flexibility |
| **LLM Provider** | Provider-agnostic (Ollama, LM Studio, OpenAI-compatible) | User choice, local-first default |

---

## Architecture

### High-Level Data Flow

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        CAPTURE LAYER (Always On)                         │
│                                                                          │
│   Screen Capture Engine (existing)                                       │
│   ├─ Multi-monitor support                                               │
│   ├─ Frame differencing (skip unchanged frames)                          │
│   ├─ JPEG compression + resizing (max 1920px width)                      │
│   └─ Store: frame metadata + JPEG image in SQLite                        │
│                                                                          │
│   Resource Usage: ~2% CPU, lightweight, runs continuously                │
└─────────────────────────────────────────────────────────────────────────┘
                                     │
                                     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                    VISION PROCESSING (User Triggered)                    │
│                                                                          │
│   User clicks "Run Vision OCR" in Dashboard                              │
│   ├─ Fetch pending frames (not yet processed)                            │
│   ├─ Send each frame to DeepSeek-VL (1.3B) via Ollama API               │
│   ├─ Extract:                                                            │
│   │   ├─ Text content (smarter than Windows OCR)                         │
│   │   ├─ Visual descriptions (colors, layouts, images)                   │
│   │   ├─ UI element classification (content vs chrome)                   │
│   │   └─ Contextual metadata (what app, what activity)                   │
│   └─ Store: vision analysis as JSON in database                          │
│                                                                          │
│   Resource Usage: Heavy (~1-2s per frame on GPU), user-controlled        │
└─────────────────────────────────────────────────────────────────────────┘
                                     │
                                     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                      INDEXATION (User Triggered)                         │
│                                                                          │
│   User clicks "Build Index" in Dashboard                                 │
│   ├─ Fetch frames with vision analysis but no embeddings                 │
│   ├─ Generate embeddings from vision output text                         │
│   │   └─ Using existing paraphrase-multilingual-MiniLM (384-dim)         │
│   └─ Store: embeddings in vector store for semantic search               │
│                                                                          │
│   Resource Usage: Medium, batched processing, user-controlled            │
└─────────────────────────────────────────────────────────────────────────┘
                                     │
                                     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                         QUERY LAYER (On Demand)                          │
│                                                                          │
│   User enters query in Dashboard (example or free-form)                  │
│   ├─ Light Mode (Quick Search): ~5-15s                                   │
│   │   ├─ Query Interpreter (Ministral-3B) extracts params                │
│   │   ├─ Hybrid search (semantic + FTS5 + time filters)                  │
│   │   └─ Response synthesizer builds Rich Report Card                    │
│   │                                                                      │
│   └─ Heavy Mode (Deep Analysis): ~30-90s                                 │
│       ├─ Query Understanding Agent decomposes intent                     │
│       ├─ Search Planning Agent designs strategies                        │
│       ├─ Parallel Retrieval (semantic, temporal, app, entity)            │
│       └─ Synthesis Agent builds comprehensive Rich Report Card           │
│                                                                          │
│   Output: Rich Report Card with summary, screenshots, timeline, context  │
└─────────────────────────────────────────────────────────────────────────┘
```

### Component Interaction Diagram

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   React UI      │────▶│   Rust API      │────▶│   SQLite DB     │
│   (Dashboard)   │◀────│   (Axum)        │◀────│   (+ FTS5)      │
└─────────────────┘     └─────────────────┘     └─────────────────┘
         │                      │                        │
         │                      ▼                        │
         │              ┌─────────────────┐              │
         │              │   Ollama API    │              │
         │              │   (Vision LLM)  │              │
         │              │   (Query LLM)   │              │
         │              └─────────────────┘              │
         │                      │                        │
         ▼                      ▼                        ▼
┌─────────────────────────────────────────────────────────────────┐
│                        Data Flow                                 │
│                                                                  │
│  1. Capture stores frames (automatic)                            │
│  2. User triggers Vision Processing → Ollama → Store analysis    │
│  3. User triggers Indexation → Generate embeddings               │
│  4. User queries → LLM interprets → Hybrid search → Report Card  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Vision Model Research & Selection

### Why Replace Windows OCR?

| Issue | Windows OCR | Vision LLM |
|-------|-------------|------------|
| **UI Chrome Noise** | Captures everything equally | Can identify and filter UI elements |
| **Visual Understanding** | Text only | Describes images, colors, layouts |
| **Context Awareness** | None | Understands what user is doing |
| **Multilingual** | Limited | Strong multilingual support |
| **Structured Output** | Raw text | JSON with metadata |

### Vision Model Comparison (Research Summary)

| Model | Size | OCR Quality | Visual Understanding | Speed | Languages | Ollama |
|-------|------|-------------|---------------------|-------|-----------|--------|
| **DeepSeek-VL** | 1.3B | Good | Strong reasoning | Fastest (~1-2s) | EN/CN/FR/DE | ✅ |
| **Moondream2** | 1.8B | Good | Good | Fast (~2-3s) | EN-primary, FR/DE | ✅ |
| **Qwen2.5-VL** | 3B | Excellent | Excellent | Medium (~4-6s) | 29 languages | ✅ |
| **SmolVLM** | 2.2B | Good (41% OCR data) | Good | Fast | Limited | ✅ |
| **Florence-2** | 0.7B | Excellent | Basic | Very Fast | Limited | ⚠️ |

### Selected: DeepSeek-VL (1.3B) as Primary

**Installation**:
```bash
ollama pull deepseek-vl:1.3b
```

**Why DeepSeek-VL**:
1. **Smallest model** (1.3B) = fastest inference, lowest resource usage
2. **MoE architecture** = efficient, only ~570M active parameters
3. **Strong reasoning** = better context understanding
4. **Multilingual** = EN/CN strongest, FR/DE supported
5. **Scientific understanding** = handles technical content well

**API Usage (Ollama)**:
```bash
# Vision request format
curl http://localhost:11434/api/generate -d '{
  "model": "deepseek-vl:1.3b",
  "prompt": "Describe what you see in this screenshot. Extract all readable text. Identify the application and what the user is doing.",
  "images": ["base64_encoded_image"],
  "stream": false
}'
```

**Expected Output Structure**:
```json
{
  "text_content": ["extracted text line 1", "extracted text line 2"],
  "visual_description": "A VS Code window showing Rust code with a dark theme",
  "application": "Visual Studio Code",
  "activity": "Writing Rust code - appears to be implementing an API handler",
  "ui_elements": {
    "content_area": "Code editor showing function implementation",
    "sidebar": "File explorer with project structure",
    "status_bar": "Branch: main, Rust language mode"
  },
  "notable_elements": ["Yellow warning squiggles on line 45", "Debug panel open"]
}
```

### Alternative Models to Test Later

**Moondream2 (1.8B)**:
```bash
ollama pull moondream
```
- Better structured output support (native JSON/XML)
- Gaze detection feature (could track user focus)
- Slightly larger but very capable

**Qwen2.5-VL-3B**:
```bash
ollama pull qwen2.5-vl:3b
```
- Best multilingual support (29 languages)
- Superior OCR quality
- Best for French/German content
- Trade-off: slower inference

---

## Dashboard UI Design

### Overall Layout

```
┌─────────────────────────────────────────────────────────────────────────┐
│  ScreenSearch                                           [Settings] [⋮]  │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                         STATS BAR                                   │ │
│  │                                                                     │ │
│  │  📷 Frames: 2,847    💾 Storage: 1.2 GB    🔍 Indexed: 1,203       │ │
│  │  ⏱️ Capture: Every 3s    🖥️ Monitors: 2                            │ │
│  │                                                                     │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                      PROCESSING CONTROLS                            │ │
│  │                                                                     │ │
│  │  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐  │ │
│  │  │  ▶ Run Vision    │  │  ▶ Build Index   │  │  ◼ Capture: ON   │  │ │
│  │  │     OCR          │  │                  │  │                  │  │ │
│  │  │  847 pending     │  │  1,644 unindexed │  │  [Pause]         │  │ │
│  │  └──────────────────┘  └──────────────────┘  └──────────────────┘  │ │
│  │                                                                     │ │
│  │  [Processing progress bar when running...]                          │ │
│  │                                                                     │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                        QUERY SECTION                                │ │
│  │                                                                     │ │
│  │  Example queries (click to use and edit):                           │ │
│  │                                                                     │ │
│  │  ┌────────────────────────────────────────────────────────────┐    │ │
│  │  │  "What was I doing on Tuesday around 11am?"                │    │ │
│  │  └────────────────────────────────────────────────────────────┘    │ │
│  │  ┌────────────────────────────────────────────────────────────┐    │ │
│  │  │  "Summarize my Reddit activity this week"                  │    │ │
│  │  └────────────────────────────────────────────────────────────┘    │ │
│  │  ┌────────────────────────────────────────────────────────────┐    │ │
│  │  │  "How long did I spend on [project] today?"                │    │ │
│  │  └────────────────────────────────────────────────────────────┘    │ │
│  │  ┌────────────────────────────────────────────────────────────┐    │ │
│  │  │  "Find that yellow design I saw earlier this week"         │    │ │
│  │  └────────────────────────────────────────────────────────────┘    │ │
│  │                                                                     │ │
│  │  Or type your own:                                                  │ │
│  │  ┌────────────────────────────────────────────────────────────┐    │ │
│  │  │  Ask anything about your screen history...              🎤 │    │ │
│  │  └────────────────────────────────────────────────────────────┘    │ │
│  │                                                                     │ │
│  │  ┌─────────────────────┐  ┌─────────────────────┐                  │ │
│  │  │  🔍 Quick Search    │  │  🧠 Deep Analysis   │                  │ │
│  │  │  Fast (~5-15s)      │  │  Thorough (~30-90s) │                  │ │
│  │  └─────────────────────┘  └─────────────────────┘                  │ │
│  │                                                                     │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                        RESULTS AREA                                 │ │
│  │                                                                     │ │
│  │  (Rich Report Cards appear here after running queries)              │ │
│  │                                                                     │ │
│  │  [Empty state: "Run a query to see results"]                        │ │
│  │                                                                     │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### Component Breakdown

#### 1. StatsBar Component
```typescript
interface StatsBarProps {
  totalFrames: number;
  storageUsed: string;        // e.g., "1.2 GB"
  indexedFrames: number;
  captureInterval: number;    // seconds
  monitorCount: number;
}
```

#### 2. ProcessingControls Component
```typescript
interface ProcessingControlsProps {
  pendingVisionFrames: number;
  unindexedFrames: number;
  captureEnabled: boolean;
  isProcessingVision: boolean;
  isProcessingIndex: boolean;
  visionProgress?: { current: number; total: number };
  indexProgress?: { current: number; total: number };
  onRunVisionOCR: () => void;
  onBuildIndex: () => void;
  onToggleCapture: () => void;
}
```

#### 3. QueryPanel Component
```typescript
interface QueryPanelProps {
  exampleQueries: string[];
  onQuerySubmit: (query: string, mode: 'light' | 'heavy') => void;
  isQuerying: boolean;
}
```

#### 4. ResultsArea Component
```typescript
interface ResultsAreaProps {
  results: ReportCard[];
  isLoading: boolean;
}
```

---

## Query System Design

### Light Mode Pipeline (Quick Search)

**Goal**: Fast answers for simple queries (~5-15s on local LLM)

```
┌─────────────────────────────────────────────────────────────────┐
│                    LIGHT MODE PIPELINE                           │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  Step 1: Query Interpretation (Ministral-3B)                     │
│                                                                  │
│  Input: "What was I doing on Tuesday around 11am?"               │
│                                                                  │
│  Prompt:                                                         │
│  """                                                             │
│  Extract search parameters from this query about screen history: │
│  Query: {user_query}                                             │
│                                                                  │
│  Return JSON with:                                               │
│  - time_range: {start: ISO, end: ISO} or null                   │
│  - apps: [list of app names mentioned] or null                  │
│  - keywords: [important search terms]                            │
│  - visual_hints: [colors, layouts, visual descriptions]         │
│  - query_type: "temporal" | "summary" | "recall" | "pattern"    │
│  """                                                             │
│                                                                  │
│  Output:                                                         │
│  {                                                               │
│    "time_range": {"start": "2024-12-10T10:30:00", "end": "..."},│
│    "apps": null,                                                 │
│    "keywords": ["doing"],                                        │
│    "visual_hints": [],                                           │
│    "query_type": "temporal"                                      │
│  }                                                               │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  Step 2: Hybrid Search                                           │
│                                                                  │
│  Execute in parallel:                                            │
│  ├─ Semantic search (embeddings) with query text                │
│  ├─ FTS5 keyword search with extracted keywords                 │
│  └─ Time-range filter if time_range provided                    │
│                                                                  │
│  Merge results:                                                  │
│  - Deduplicate by frame_id                                       │
│  - Score = 0.3 * semantic + 0.7 * keyword (configurable)        │
│  - Apply recency boost                                           │
│  - Return top 20 results                                         │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  Step 3: Response Synthesis (Ministral-3B)                       │
│                                                                  │
│  Prompt:                                                         │
│  """                                                             │
│  Based on this screen activity context, answer the user's query. │
│                                                                  │
│  User Query: {original_query}                                    │
│                                                                  │
│  Screen Activity Context:                                        │
│  {formatted_search_results}                                      │
│                                                                  │
│  Provide:                                                        │
│  1. A 2-3 sentence summary answering the query                   │
│  2. Key timestamps and events                                    │
│  3. Applications used and time spent                             │
│  4. Any relevant visual or contextual details                    │
│  """                                                             │
│                                                                  │
│  Output: Structured response for Rich Report Card                │
└─────────────────────────────────────────────────────────────────┘
```

### Heavy Mode Pipeline (Deep Analysis)

**Goal**: Comprehensive analysis for complex queries (~30-90s on local LLM)

```
┌─────────────────────────────────────────────────────────────────┐
│                    HEAVY MODE PIPELINE                           │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  Agent 1: Query Understanding                                    │
│                                                                  │
│  Responsibilities:                                               │
│  - Decompose complex/multi-part queries                          │
│  - Identify implicit requirements                                │
│  - Determine what information is needed                          │
│  - Flag ambiguities for clarification                            │
│                                                                  │
│  Example:                                                        │
│  Input: "How much time did I spend on the ScreenSearch project   │
│          this week, and what were the main activities?"          │
│                                                                  │
│  Output:                                                         │
│  {                                                               │
│    "sub_queries": [                                              │
│      "Find all frames where ScreenSearch project was active",    │
│      "Calculate time spent on ScreenSearch",                     │
│      "Categorize activities (coding, testing, docs, etc.)"       │
│    ],                                                            │
│    "time_scope": "this_week",                                    │
│    "requires": ["app_time_tracking", "activity_categorization"]  │
│  }                                                               │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  Agent 2: Search Planning                                        │
│                                                                  │
│  Responsibilities:                                               │
│  - Design search strategies for each sub-query                   │
│  - Decide which search types to use                              │
│  - Plan parallel vs sequential execution                         │
│  - Identify potential related queries to explore                 │
│                                                                  │
│  Output:                                                         │
│  {                                                               │
│    "search_plan": [                                              │
│      {                                                           │
│        "type": "semantic",                                       │
│        "query": "ScreenSearch project development",              │
│        "filters": {"time": "this_week"}                          │
│      },                                                          │
│      {                                                           │
│        "type": "app_based",                                      │
│        "apps": ["VS Code", "Terminal", "Chrome"],                │
│        "pattern": "screensearch|screen-search"                   │
│      },                                                          │
│      {                                                           │
│        "type": "keyword",                                        │
│        "terms": ["screensearch", "cargo", "rust"]                │
│      }                                                           │
│    ],                                                            │
│    "execution": "parallel"                                       │
│  }                                                               │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  Step 3: Parallel Retrieval                                      │
│                                                                  │
│  Execute all search strategies concurrently:                     │
│                                                                  │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐              │
│  │  Semantic   │  │  App-based  │  │  Keyword    │              │
│  │  Search     │  │  Search     │  │  Search     │              │
│  └─────────────┘  └─────────────┘  └─────────────┘              │
│         │               │               │                        │
│         └───────────────┴───────────────┘                        │
│                         │                                        │
│                         ▼                                        │
│              ┌─────────────────────┐                             │
│              │  Result Aggregation │                             │
│              │  - Deduplicate      │                             │
│              │  - Cross-reference  │                             │
│              │  - Rank by relevance│                             │
│              └─────────────────────┘                             │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  Agent 3: Synthesis                                              │
│                                                                  │
│  Responsibilities:                                               │
│  - Analyze aggregated results                                    │
│  - Build comprehensive timeline                                  │
│  - Calculate statistics (time spent, app breakdown)              │
│  - Generate insights and patterns                                │
│  - Format Rich Report Card                                       │
│                                                                  │
│  Output: Complete Rich Report Card with all sections             │
└─────────────────────────────────────────────────────────────────┘
```

---

## Rich Report Card Format

### Structure

```typescript
interface ReportCard {
  id: string;
  query: string;
  mode: 'light' | 'heavy';
  generatedAt: string;

  summary: {
    text: string;           // 2-3 sentence answer
    confidence: number;     // 0-1 confidence score
  };

  screenshots: {
    frameId: number;
    timestamp: string;
    thumbnailUrl: string;
    appName: string;
    relevanceScore: number;
  }[];

  timeline: {
    time: string;
    event: string;
    appName?: string;
    frameId?: number;
  }[];

  appBreakdown: {
    appName: string;
    duration: number;       // minutes
    percentage: number;
  }[];

  relatedContext: {
    type: 'text' | 'visual' | 'entity';
    value: string;
    source: string;         // where it was found
  }[];

  metadata: {
    framesAnalyzed: number;
    searchTime: number;     // ms
    llmCalls: number;
  };
}
```

### Visual Design

```
┌─────────────────────────────────────────────────────────────────────────┐
│  📊 QUERY RESULT                                        [Copy] [Save]   │
│  Query: "What was I doing on Tuesday around 11am?"                      │
│  Mode: Quick Search • Generated: 2024-12-14 14:32:05                    │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ## Summary                                                              │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │ On Tuesday around 11am, you were reviewing a brand design in       │ │
│  │ Figma called "SunBurst UI Kit". You spent approximately 25         │ │
│  │ minutes iterating on the color palette and typography before       │ │
│  │ sharing a preview link in Slack with your design team.             │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ## Key Screenshots                                                      │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌────────────┐        │
│  │            │  │            │  │            │  │            │        │
│  │  [thumb]   │  │  [thumb]   │  │  [thumb]   │  │  [thumb]   │        │
│  │            │  │            │  │            │  │            │        │
│  │ 10:47 AM   │  │ 11:02 AM   │  │ 11:15 AM   │  │ 11:23 AM   │        │
│  │ Figma      │  │ Figma      │  │ Chrome     │  │ Slack      │        │
│  └────────────┘  └────────────┘  └────────────┘  └────────────┘        │
│                                                                          │
│  ## Timeline                                                             │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │ 10:30  Opened Figma project "SunBurst UI Kit"                      │ │
│  │ 10:47  Focused on color palette - yellow (#FFD700) variants        │ │
│  │ 11:02  Adjusted typography settings in design system               │ │
│  │ 11:15  Exported preview images to Desktop folder                   │ │
│  │ 11:18  Opened Slack, navigated to #design channel                  │ │
│  │ 11:23  Shared Figma preview link, received feedback from @marie    │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ## Apps Used                                                            │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │ Figma ████████████████████████░░░░░░  25 min (69%)                 │ │
│  │ Slack ████████░░░░░░░░░░░░░░░░░░░░░░   8 min (22%)                 │ │
│  │ Chrome ███░░░░░░░░░░░░░░░░░░░░░░░░░░   3 min (9%)                  │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ## Related Context                                                      │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │ • Project: "SunBurst UI Kit" (Figma)                               │ │
│  │ • Team: @marie, @design-team (Slack)                               │ │
│  │ • File exported: sunburst-preview.png (Desktop)                    │ │
│  │ • Visual: Yellow/gold color theme, modern typography               │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ─────────────────────────────────────────────────────────────────────  │
│  📈 12 frames analyzed • Search: 847ms • 2 LLM calls                    │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## API Design

### New Endpoints

#### Vision Processing

```
POST /api/vision/process
```
Trigger vision OCR processing on pending frames.

**Request**:
```json
{
  "batch_size": 50,           // Optional, default 50
  "frame_ids": [1, 2, 3]      // Optional, specific frames to process
}
```

**Response**:
```json
{
  "job_id": "vision-job-123",
  "status": "started",
  "total_frames": 847,
  "estimated_time_seconds": 1694
}
```

---

```
GET /api/vision/status
```
Get current vision processing status.

**Response**:
```json
{
  "is_processing": true,
  "job_id": "vision-job-123",
  "progress": {
    "current": 234,
    "total": 847,
    "percentage": 27.6
  },
  "pending_frames": 613,
  "processed_today": 234,
  "errors": []
}
```

---

#### Query Endpoints

```
POST /api/query/light
```
Execute light mode (quick search) query.

**Request**:
```json
{
  "query": "What was I doing on Tuesday around 11am?",
  "provider_url": "http://localhost:11434/v1",
  "model": "ministral-3b",
  "api_key": ""
}
```

**Response**:
```json
{
  "report_card": {
    "id": "report-456",
    "query": "What was I doing on Tuesday around 11am?",
    "mode": "light",
    "generated_at": "2024-12-14T14:32:05Z",
    "summary": {
      "text": "On Tuesday around 11am...",
      "confidence": 0.87
    },
    "screenshots": [...],
    "timeline": [...],
    "app_breakdown": [...],
    "related_context": [...],
    "metadata": {
      "frames_analyzed": 12,
      "search_time_ms": 847,
      "llm_calls": 2
    }
  }
}
```

---

```
POST /api/query/deep
```
Execute heavy mode (deep analysis) query.

**Request**:
```json
{
  "query": "How much time did I spend on ScreenSearch this week?",
  "provider_url": "http://localhost:11434/v1",
  "model": "ministral-3b",
  "api_key": ""
}
```

**Response**: Same structure as light mode, but with more comprehensive data.

---

#### Dashboard Stats

```
GET /api/stats
```
Get dashboard statistics.

**Response**:
```json
{
  "frames": {
    "total": 2847,
    "pending_vision": 847,
    "vision_processed": 2000,
    "indexed": 1203,
    "unindexed": 1644
  },
  "storage": {
    "total_bytes": 1288490188,
    "formatted": "1.2 GB"
  },
  "capture": {
    "enabled": true,
    "interval_seconds": 3,
    "monitor_count": 2
  },
  "processing": {
    "vision_in_progress": false,
    "index_in_progress": false
  }
}
```

---

### Modified Endpoints

```
GET /api/frames
```
Add filter for vision processing status.

**New Query Parameters**:
- `vision_processed`: `true` | `false` | `all` (default: `all`)
- `indexed`: `true` | `false` | `all` (default: `all`)

---

## Database Schema Changes

### New Tables

```sql
-- Vision analysis results from Vision LLM
CREATE TABLE vision_analysis (
    id INTEGER PRIMARY KEY,
    frame_id INTEGER NOT NULL UNIQUE,
    model_used TEXT NOT NULL,              -- e.g., "deepseek-vl:1.3b"

    -- Extracted content
    text_content TEXT,                      -- JSON array of extracted text
    visual_description TEXT,                -- Natural language description
    application_detected TEXT,              -- Detected app name
    activity_detected TEXT,                 -- What user appears to be doing
    ui_elements TEXT,                       -- JSON of UI element breakdown
    notable_elements TEXT,                  -- JSON array of notable visual elements

    -- Raw response
    raw_response TEXT,                      -- Full LLM response for debugging

    -- Metadata
    processing_time_ms INTEGER,
    confidence_score REAL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (frame_id) REFERENCES frames(id) ON DELETE CASCADE
);

-- Index for efficient queries
CREATE INDEX idx_vision_analysis_frame_id ON vision_analysis(frame_id);
CREATE INDEX idx_vision_analysis_created_at ON vision_analysis(created_at);

-- FTS5 for vision text search
CREATE VIRTUAL TABLE vision_text_fts USING fts5(
    text_content,
    visual_description,
    activity_detected,
    content='vision_analysis',
    content_rowid='id',
    tokenize = 'porter'
);
```

### Schema Migration

```sql
-- Migration: Add vision processing status to frames
ALTER TABLE frames ADD COLUMN vision_processed INTEGER DEFAULT 0;
ALTER TABLE frames ADD COLUMN vision_processed_at DATETIME;

-- Index for filtering
CREATE INDEX idx_frames_vision_processed ON frames(vision_processed);
```

---

## Implementation Phases

### Phase 1: Foundation (Vision + Dashboard Shell)

**Duration**: 1-2 weeks
**Goal**: Get basic vision processing working and new Dashboard visible

**Tasks**:

1. **Backend: Vision Processing Handler**
   - Create `screensearch-api/src/handlers/vision.rs`
   - Implement Ollama API client for vision models
   - Add batch processing with progress tracking
   - Store results in new `vision_analysis` table

2. **Backend: Stats Endpoint**
   - Create `/api/stats` endpoint
   - Aggregate frame counts, storage, processing status

3. **Database: Schema Updates**
   - Add `vision_analysis` table
   - Add `vision_processed` column to frames
   - Create FTS5 table for vision text

4. **Frontend: Dashboard Shell**
   - Create new `Dashboard.tsx` page
   - Implement `StatsBar` component
   - Implement `ProcessingControls` component
   - Wire up to new API endpoints

5. **Frontend: Remove Broken Features**
   - Remove or hide broken Timeline/Search pages
   - Update navigation to use new Dashboard

**Deliverable**: User can trigger Vision OCR processing and see stats.

---

### Phase 2: Query Intelligence

**Duration**: 1-2 weeks
**Goal**: Natural language queries with Rich Report Cards

**Tasks**:

1. **Backend: Light Mode Query Pipeline**
   - Implement query interpretation prompt
   - Integrate with existing hybrid search
   - Build response synthesis prompt
   - Create `/api/query/light` endpoint

2. **Backend: Report Card Generation**
   - Build report card data structure
   - Aggregate screenshots, timeline, app breakdown
   - Extract related context from results

3. **Frontend: Query Panel**
   - Implement example queries (clickable, editable)
   - Add free-form query input
   - Add mode toggle (Quick/Deep)
   - Show loading state during queries

4. **Frontend: Report Card Component**
   - Create `ReportCard.tsx` component
   - Implement all sections (summary, screenshots, timeline, etc.)
   - Add copy/save functionality

**Deliverable**: Users can run natural language queries and see Rich Report Cards.

---

### Phase 3: Heavy Mode + Polish

**Duration**: 1-2 weeks
**Goal**: Multi-agent deep analysis and refinements

**Tasks**:

1. **Backend: Heavy Mode Pipeline**
   - Implement Query Understanding Agent
   - Implement Search Planning Agent
   - Implement parallel retrieval
   - Implement Synthesis Agent
   - Create `/api/query/deep` endpoint

2. **Frontend: Progress Indicators**
   - Show detailed progress for heavy queries
   - Display agent steps as they complete

3. **Vision Model Options**
   - Add settings for alternative models
   - Test Moondream2 and Qwen2.5-VL
   - Allow user model selection

4. **Polish & Testing**
   - Refine prompts based on real usage
   - Optimize search performance
   - Add error handling and recovery
   - Write integration tests

**Deliverable**: Full system with both query modes working reliably.

---

## File-by-File Implementation Guide

### Backend (Rust)

| File | Purpose | Key Changes |
|------|---------|-------------|
| `screensearch-api/src/handlers/vision.rs` | **NEW** | Vision processing handler, Ollama client |
| `screensearch-api/src/handlers/query.rs` | **NEW** | Light/Heavy query handlers |
| `screensearch-api/src/handlers/stats.rs` | **NEW** | Dashboard stats handler |
| `screensearch-api/src/routes.rs` | Routing | Add new endpoints |
| `screensearch-api/src/models.rs` | Types | Add VisionAnalysis, ReportCard types |
| `screensearch-db/src/migrations.rs` | Schema | Add vision_analysis table |
| `screensearch-db/src/queries.rs` | Queries | Add vision-related queries |

### Frontend (React/TypeScript)

| File | Purpose | Key Changes |
|------|---------|-------------|
| `screensearch-ui/src/pages/Dashboard.tsx` | **NEW** | Main dashboard page |
| `screensearch-ui/src/components/StatsBar.tsx` | **NEW** | Stats display |
| `screensearch-ui/src/components/ProcessingControls.tsx` | **NEW** | OCR/Index triggers |
| `screensearch-ui/src/components/QueryPanel.tsx` | **NEW** | Query input UI |
| `screensearch-ui/src/components/ReportCard.tsx` | **NEW** | Results display |
| `screensearch-ui/src/api/vision.ts` | **NEW** | Vision API client |
| `screensearch-ui/src/api/query.ts` | **NEW** | Query API client |
| `screensearch-ui/src/api/stats.ts` | **NEW** | Stats API client |
| `screensearch-ui/src/hooks/useStats.ts` | **NEW** | Stats data hook |
| `screensearch-ui/src/hooks/useVision.ts` | **NEW** | Vision processing hook |
| `screensearch-ui/src/hooks/useQuery.ts` | **NEW** | Query execution hook |
| `screensearch-ui/src/store/useStore.ts` | State | Add vision/query state |
| `screensearch-ui/src/App.tsx` | Routing | Update navigation |

---

## Testing Strategy

### Unit Tests

- Query interpretation prompt parsing
- Report card data aggregation
- Time range extraction from queries
- Search result ranking

### Integration Tests

- Vision processing pipeline (with mock Ollama)
- Light mode query end-to-end
- Heavy mode query end-to-end
- Dashboard stats accuracy

### Manual Testing Scenarios

1. **Vision OCR Flow**
   - Capture 100 frames
   - Trigger Vision OCR
   - Verify text extraction quality
   - Check for UI chrome filtering

2. **Query Scenarios**
   - Temporal: "What was I doing at 3pm yesterday?"
   - App-based: "Show my VS Code activity this week"
   - Content: "Find that error message I saw"
   - Visual: "Find the red notification badge"
   - Pattern: "How much time on email today?"

3. **Edge Cases**
   - Empty results
   - Very old timestamps
   - Ambiguous queries
   - Long processing times

---

## Open Questions

### Technical

1. **Frame Batching**: How many frames per Vision OCR batch?
   - Trade-off: Larger batches = less overhead, but longer before first results
   - Recommendation: Start with 50, make configurable

2. **Storage Format**: How to store Moondream output?
   - Option A: JSON column in existing `ocr_text` table
   - Option B: New `vision_analysis` table (recommended for cleaner separation)
   - Option C: Replace `ocr_text` entirely

3. **Embedding Model**: Keep current paraphrase-multilingual-MiniLM?
   - Vision output is richer text, might benefit from different model
   - For now: Keep current model, evaluate after initial testing

4. **Progress Updates**: Polling vs WebSocket?
   - Polling: Simpler, works everywhere
   - WebSocket: Real-time, better UX
   - Recommendation: Start with polling, add WebSocket later

### Product

1. **Example Queries**: What are the best default examples?
   - Should cover: temporal, app-based, content, visual, pattern
   - Should be relatable and demonstrate capabilities

2. **Error Handling**: What to show when queries fail?
   - LLM unavailable
   - No results found
   - Ambiguous query

3. **History**: Should we save query history?
   - Could help users re-run common queries
   - Privacy consideration: storing queries

---

## References & Research

### Vision Models

- **DeepSeek-VL**: https://github.com/deepseek-ai/DeepSeek-VL
- **Moondream2**: https://github.com/vikhyat/moondream
- **Qwen2.5-VL**: https://qwenlm.github.io/blog/qwen2.5-vl/
- **Vision Model Comparison 2025**: https://www.labellerr.com/blog/top-open-source-vision-language-models/
- **Trelis Research VLM Analysis**: https://trelis.substack.com/p/top-vision-models-2025

### Current Codebase

- **OCR Implementation**: `screensearch-capture/src/ocr.rs`
- **AI Handlers**: `screensearch-api/src/handlers/ai.rs`
- **RAG Helpers**: `screensearch-api/src/rag_helpers.rs`
- **UI Components**: `screensearch-ui/src/components/`
- **Database Queries**: `screensearch-db/src/queries.rs`

### Related Technologies

- **Ollama**: https://ollama.ai/
- **LM Studio**: https://lmstudio.ai/
- **FTS5 Full-Text Search**: https://sqlite.org/fts5.html

---

## Changelog

| Date | Change |
|------|--------|
| 2024-12-14 | Initial design document created |
| 2024-12-14 | Selected DeepSeek-VL (1.3B) as primary vision model |
| 2024-12-14 | Defined Dashboard UI and Rich Report Card formats |
| 2024-12-14 | Documented Light and Heavy query pipelines |
