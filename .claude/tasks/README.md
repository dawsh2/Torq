# Task Organization

## Structure

```
tasks/
├── README.md              # This file
├── index.org             # Master index with links to projects
├── projects/             # Active project files
│   ├── arc-types.org     # Arc/Types Internal/Wire architecture
│   ├── mycelium.org      # Mycelium messaging library
│   ├── actors.org        # Lightweight actor runtime
│   ├── flash-arb.org     # Flash arbitrage E2E
│   └── build-fixes.org   # Critical build fixes
├── backlog/              # Future work
│   ├── performance.org   # Performance improvements
│   └── cleanup.org       # Code cleanup tasks
└── archive/              # Completed sprints (already exists)
```

## Active Projects

1. **Critical/Blocking** (Priority A)
   - `projects/build-fixes.org` - Fix compilation errors
   - `projects/arc-types.org` - Internal/Wire message separation

2. **Core Infrastructure** (Priority B)  
   - `projects/mycelium.org` - Messaging library
   - `projects/actors.org` - Actor runtime

3. **Business Logic** (Priority A)
   - `projects/flash-arb.org` - End-to-end arbitrage flow

## Usage

- Each project file should be <500 lines for agent processing
- Use `index.org` to navigate between projects
- Move completed projects to `archive/` with date prefix