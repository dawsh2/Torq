#!/usr/bin/env python3
"""Bridge between org-mode task system and cc-sessions protocols.

This adapter allows cc-sessions protocols to work with org-mode tasks
while preserving DAG dependencies and scrum-leader agent compatibility.
"""

import json
import re
import subprocess
from pathlib import Path
from datetime import datetime
from typing import Dict, Optional, List, Tuple

class OrgModeBridge:
    """Adapts org-mode tasks for cc-sessions compatibility."""
    
    def __init__(self):
        self.project_root = Path(__file__).parent.parent.parent
        self.tasks_dir = self.project_root / ".claude" / "tasks"
        self.state_dir = self.project_root / ".claude" / "state"
        self.index_file = self.tasks_dir / "index.org"
        
    def get_current_task(self) -> Optional[Dict]:
        """Read current task from cc-sessions state."""
        state_file = self.state_dir / "current_task.json"
        if state_file.exists():
            with open(state_file) as f:
                return json.load(f)
        return None
    
    def find_org_task(self, task_name: str) -> Optional[Tuple[str, str]]:
        """Find org-mode task by name, return (file_path, task_id)."""
        # First check index.org
        if self.index_file.exists():
            with open(self.index_file) as f:
                content = f.read()
                # Look for task ID in properties
                id_pattern = rf':ID:\s+({re.escape(task_name)})'
                match = re.search(id_pattern, content, re.MULTILINE)
                if match:
                    return (str(self.index_file), match.group(1))
                
                # Also check in headings
                heading_pattern = rf'\*+.*{re.escape(task_name)}'
                if re.search(heading_pattern, content):
                    return (str(self.index_file), task_name)
        
        # Check project files
        for org_file in self.tasks_dir.glob("projects/*.org"):
            with open(org_file) as f:
                content = f.read()
                if task_name in content:
                    return (str(org_file), task_name)
        
        return None
    
    def org_to_markdown_context(self, org_file: str, task_id: str) -> str:
        """Convert org-mode task to markdown format for cc-sessions agents."""
        with open(org_file) as f:
            lines = f.readlines()
        
        markdown_content = []
        in_task = False
        task_level = 0
        
        for line in lines:
            # Detect task start
            if f":ID:" in line and task_id in line:
                in_task = True
                # Find the heading above this ID
                for i in range(len(lines)):
                    if lines[i] == line:
                        # Go backwards to find the heading
                        for j in range(i-1, -1, -1):
                            if lines[j].startswith("*"):
                                task_level = lines[j].count("*")
                                # Convert org heading to markdown
                                heading = lines[j].replace("*", "#" * min(task_level, 6))
                                markdown_content.append(heading)
                                break
                        break
                continue
            
            if in_task:
                # Stop at next task of same or higher level
                if line.startswith("*" * task_level) and line != lines[0]:
                    break
                
                # Convert org-mode syntax to markdown
                if line.startswith("*"):
                    # Convert heading
                    level = line.count("*")
                    line = line.replace("*", "#" * min(level, 6))
                elif line.strip().startswith("- [ ]"):
                    # Checkbox remains the same
                    pass
                elif line.strip().startswith("- [X]"):
                    # Completed checkbox
                    pass
                elif ":PROPERTIES:" in line or ":END:" in line:
                    # Skip org-mode properties
                    continue
                elif line.strip().startswith(":"):
                    # Skip property lines
                    continue
                
                markdown_content.append(line)
        
        return "".join(markdown_content)
    
    def update_org_task_status(self, task_id: str, new_status: str) -> bool:
        """Update org-mode task status (TODO, IN-PROGRESS, DONE)."""
        org_file, _ = self.find_org_task(task_id) or (None, None)
        if not org_file:
            return False
        
        with open(org_file) as f:
            content = f.read()
        
        # Map cc-sessions status to org-mode
        status_map = {
            "pending": "TODO",
            "in_progress": "IN-PROGRESS", 
            "completed": "DONE"
        }
        
        org_status = status_map.get(new_status, "TODO")
        
        # Update the task status
        pattern = rf'(\*+\s+)(TODO|IN-PROGRESS|DONE|NEXT)(\s+\[#[A-Z]\]\s+.*{re.escape(task_id)})'
        replacement = rf'\1{org_status}\3'
        
        new_content = re.sub(pattern, replacement, content)
        
        if new_content != content:
            with open(org_file, 'w') as f:
                f.write(new_content)
            return True
        
        return False
    
    def sync_to_sessions_state(self, task_name: str, branch: str = None) -> Dict:
        """Create/update cc-sessions state from org-mode task."""
        org_file, task_id = self.find_org_task(task_name) or (None, None)
        
        if not branch:
            # Infer branch from task name
            if task_name.startswith("fix-"):
                branch = f"fix/{task_name}"
            elif task_name.startswith("refactor-"):
                branch = f"refactor/{task_name}"
            else:
                branch = f"feature/{task_name}"
        
        state = {
            "task": task_name,
            "branch": branch,
            "services": self.extract_services_from_org(org_file, task_id) if org_file else [],
            "updated": datetime.now().strftime("%Y-%m-%d"),
            "org_file": org_file,
            "org_task_id": task_id
        }
        
        # Save to cc-sessions state
        self.state_dir.mkdir(exist_ok=True)
        with open(self.state_dir / "current_task.json", 'w') as f:
            json.dump(state, f, indent=2)
        
        return state
    
    def extract_services_from_org(self, org_file: str, task_id: str) -> List[str]:
        """Extract affected services from org-mode task tags."""
        if not org_file:
            return []
        
        with open(org_file) as f:
            content = f.read()
        
        # Look for tags in the task line
        pattern = rf'\*+.*{re.escape(task_id)}.*:([\w:]+):'
        match = re.search(pattern, content)
        
        if match:
            tags = match.group(1).split(":")
            # Filter for service-related tags
            services = [tag for tag in tags if tag in [
                "flash-arbitrage", "polygon-adapter", "market-data",
                "execution", "strategies", "protocol", "codec"
            ]]
            return services
        
        return []
    
    def create_markdown_task_file(self, task_name: str) -> Path:
        """Create a markdown task file from org-mode for cc-sessions agents."""
        org_file, task_id = self.find_org_task(task_name) or (None, None)
        
        if not org_file:
            return None
        
        # Convert to markdown
        markdown_content = self.org_to_markdown_context(org_file, task_id)
        
        # Create sessions task directory if needed
        sessions_tasks = self.project_root / "sessions" / "tasks"
        sessions_tasks.mkdir(parents=True, exist_ok=True)
        
        # Write markdown file
        md_file = sessions_tasks / f"{task_name}.md"
        with open(md_file, 'w') as f:
            f.write(markdown_content)
        
        return md_file


def main():
    """CLI interface for testing the bridge."""
    import sys
    
    bridge = OrgModeBridge()
    
    if len(sys.argv) < 2:
        print("Usage: org-mode-bridge.py <command> [args]")
        print("Commands:")
        print("  current - Show current task")
        print("  find <task> - Find org task")
        print("  sync <task> [branch] - Sync to sessions state")
        print("  convert <task> - Convert to markdown")
        return
    
    command = sys.argv[1]
    
    if command == "current":
        task = bridge.get_current_task()
        print(json.dumps(task, indent=2) if task else "No current task")
    
    elif command == "find" and len(sys.argv) > 2:
        result = bridge.find_org_task(sys.argv[2])
        if result:
            print(f"Found: {result[0]} (ID: {result[1]})")
        else:
            print("Task not found")
    
    elif command == "sync" and len(sys.argv) > 2:
        branch = sys.argv[3] if len(sys.argv) > 3 else None
        state = bridge.sync_to_sessions_state(sys.argv[2], branch)
        print(json.dumps(state, indent=2))
    
    elif command == "convert" and len(sys.argv) > 2:
        md_file = bridge.create_markdown_task_file(sys.argv[2])
        if md_file:
            print(f"Created: {md_file}")
        else:
            print("Failed to convert task")


if __name__ == "__main__":
    main()