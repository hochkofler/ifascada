import { Component, OnInit, OnDestroy } from '@angular/core';
import { CommonModule } from '@angular/common';
import { ScadaService, AgentData } from '../../services/scada.service';
import { SseService, ScadaEvent } from '../../services/sse.service';
import { Subscription } from 'rxjs';
import { TagHistoryComponent } from '../tag-history/tag-history.component';

@Component({
  selector: 'app-monitor',
  standalone: true,
  imports: [CommonModule, TagHistoryComponent],
  template: `
    <div class="monitor-layout">
      <div class="monitor-container">
        <header>
          <h1>Live Monitoring</h1>
          <div class="status-summary">
            Active Agents: {{ onlineCount }} / {{ agents.length }}
          </div>
        </header>

        <div class="agent-grid">
          <div *ngFor="let agent of agents" class="agent-card" 
               [class.offline]="agent.status !== 'Online'"
               [class.unregistered]="!agent.is_registered">
            <div class="agent-header">
              <div class="agent-title">
                <h3>{{ agent.id }}</h3>
                <span *ngIf="!agent.is_registered" class="unregistered-badge">No Registrado</span>
              </div>
              <span class="status-indicator" [class.online]="agent.status === 'Online'"></span>
            </div>
            
            <div class="agent-stats" *ngIf="agent.metrics">
              <span class="stat">Uptime: {{ formatUptime(agent.metrics.uptime) }}</span>
              <span class="stat">Active Tags: {{ agent.metrics.tags }}</span>
            </div>
            
            <div class="tag-list">
              <div *ngFor="let tag of getAgentTags(agent.id)" class="tag-item" [class.selected]="selectedTagId === tag.id" (click)="selectTag(tag.id, agent.id)">
                <div class="tag-info">
                   <div class="tag-name-row">
                     <span class="tag-status" [class.online]="isTagOnline(tag, agent)"></span>
                     <span class="tag-name">{{ tag.id }}</span>
                   </div>
                   <span class="tag-meta" *ngIf="!tag.quality">(esperando datos)</span>
                </div>
                <div class="tag-values" *ngIf="tag.quality">
                  <span class="tag-value">{{ formatTagValue(tag) }}</span>
                  <span class="tag-quality" [class.good]="tag.quality === 'Good'">{{ tag.quality }}</span>
                </div>
                <div class="tag-action">
                    <span class="history-link">History →</span>
                </div>
              </div>
              <div *ngIf="getAgentTags(agent.id).length === 0 && (!agent.metrics || agent.metrics.tags === 0)" class="no-tags">
                No active tags
              </div>
              <div *ngIf="getAgentTags(agent.id).length === 0 && agent.metrics && agent.metrics.tags > 0" class="waiting-tags">
                Waiting for heartbeat data ({{ agent.metrics.tags }} active)
              </div>
            </div>

            <div class="agent-footer">
              Last seen: {{ agent.last_seen | date:'HH:mm:ss' }}
            </div>
          </div>
        </div>
      </div>

      <aside class="history-panel" *ngIf="selectedTagId">
        <app-tag-history [tagId]="selectedTagId" [agentId]="selectedAgentId" [closeCallback]="closeHistory.bind(this)"></app-tag-history>
      </aside>
    </div>
  `,
  styles: [`
    .monitor-layout { display: flex; height: 100%; overflow: hidden; }
    .monitor-container { flex: 1; padding: 20px; color: #e2e8f0; overflow-y: auto; }
    header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 30px; }
    h1 { margin: 0; font-weight: 300; letter-spacing: 1px; }
    
    .history-panel { width: 450px; padding: 20px; border-left: 1px solid rgba(255,255,255,0.05); background: rgba(15, 23, 42, 0.4); overflow-y: auto; }

    .agent-grid {
      display: grid;
      grid-template-columns: repeat(auto-fill, minmax(350px, 1fr));
      gap: 20px;
    }

    .agent-card {
      background: rgba(30, 41, 59, 0.7);
      backdrop-filter: blur(10px);
      border: 1px solid rgba(255, 255, 255, 0.1);
      border-radius: 12px;
      padding: 20px;
      transition: transform 0.2s, box-shadow 0.2s;
    }

    .agent-card:hover { transform: translateY(-5px); box-shadow: 0 10px 20px rgba(0,0,0,0.3); }
    .agent-card.offline { opacity: 0.6; }
    .agent-card.unregistered { border: 1px dashed rgba(239, 68, 68, 0.5); background: rgba(30, 41, 59, 0.4); }

    .agent-header { display: flex; justify-content: space-between; align-items: flex-start; margin-bottom: 15px; }
    .agent-title h3 { margin: 0; }
    
    .unregistered-badge { 
      font-size: 0.65em; background: #991b1b; color: #fecaca; padding: 2px 6px; border-radius: 4px; text-transform: uppercase; font-weight: bold; margin-top: 4px; display: inline-block;
    }

    .status-indicator { width: 10px; height: 10px; border-radius: 50%; background: #ef4444; margin-top: 6px; }
    .status-indicator.online { background: #10b981; box-shadow: 0 0 10px #10b981; }

    .tag-list { border-top: 1px solid rgba(255,255,255,0.05); padding-top: 15px; }
    .tag-item { 
      display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px; padding: 8px; border-radius: 6px; 
      font-family: 'JetBrains Mono', monospace; font-size: 0.85em; cursor: pointer; transition: background 0.2s;
      background: rgba(255,255,255,0.02);
    }
    .tag-item:hover { background: rgba(59, 130, 246, 0.1); }
    .tag-item.selected { background: rgba(59, 130, 246, 0.2); border: 1px solid rgba(59, 130, 246, 0.4); }
    
    .tag-info { display: flex; flex-direction: column; }
    .tag-name-row { display: flex; align-items: center; gap: 8px; }
    .tag-status { width: 6px; height: 6px; border-radius: 50%; background: #64748b; }
    .tag-status.online { background: #10b981; box-shadow: 0 0 5px #10b981; }
    .tag-name { color: #94a3b8; }
    .tag-meta { font-size: 0.8em; color: #64748b; margin-left:14px; }

    .tag-values { display: flex; gap: 8px; align-items: center; }
    .tag-value { font-weight: bold; color: #60a5fa; }
    .tag-quality { font-size: 0.75em; padding: 2px 6px; border-radius: 4px; background: #334155; }
    .tag-quality.good { color: #10b981; }

    .history-link { font-size: 0.75em; color: #3b82f6; opacity: 0; transition: opacity 0.2s; }
    .tag-item:hover .history-link { opacity: 1; }

    .agent-footer { margin-top: 15px; font-size: 0.75em; color: #64748b; text-align: right; }
    .no-tags { font-size: 0.8em; color: #64748b; font-style: italic; }
    .waiting-tags { font-size: 0.8em; color: #3b82f6; font-style: italic; margin-top: 5px; }

    .agent-stats {
      display: flex;
      flex-direction: column;
      gap: 4px;
      margin-bottom: 15px;
      padding: 10px;
      background: rgba(0,0,0,0.2);
      border-radius: 8px;
    }
    .stat {
      font-size: 0.8em;
      color: #94a3b8;
      font-family: 'JetBrains Mono', monospace;
    }
  `]
})
export class MonitorComponent implements OnInit, OnDestroy {
  agents: AgentData[] = [];
  tags: Map<string, any> = new Map(); // Global tag state
  selectedTagId: string | null = null;
  selectedAgentId: string = '';
  private sub: Subscription | null = null;

  constructor(private scada: ScadaService, private sse: SseService) { }

  ngOnInit() {
    // 1. Load initial agents
    this.scada.getAgents().subscribe(data => this.agents = data);

    // 2. Load all configured tags
    this.scada.getTags().subscribe(data => {
      data.forEach(tag => {
        this.tags.set(tag.id, tag);
      });
    });

    // 3. Listen for live updates
    this.sub = this.sse.getEvents().subscribe(event => this.handleEvent(event));
  }

  ngOnDestroy() {
    this.sub?.unsubscribe();
  }

  get onlineCount() {
    return this.agents.filter(a => a.status === 'Online').length;
  }

  getAgentTags(agentId: string) {
    return Array.from(this.tags.values()).filter(t => t.agent_id === agentId);
  }

  selectTag(tagId: string, agentId: string) {
    this.selectedTagId = tagId;
    this.selectedAgentId = agentId;
  }

  closeHistory() {
    this.selectedTagId = null;
  }

  formatTagValue(tag: any): string {
    const val = tag.last_value || tag.value;
    if (val === undefined || val === null) return '---';

    if (tag.value_type === 'Simple') {
      const unit = tag.value_schema?.unit || '';
      return `${val} ${unit}`.trim();
    } else if (tag.value_type === 'Composite') {
      if (typeof val === 'object') {
        const parts: string[] = [];
        const schema = tag.value_schema || {};
        const labels = schema.labels || {};

        for (const [key, value] of Object.entries(val)) {
          const label = labels[key] || key;
          parts.push(`${label}: ${value}`);
        }
        return parts.length > 0 ? parts.join(', ') : JSON.stringify(val);
      }
      return JSON.stringify(val);
    }

    return typeof val === 'object' ? JSON.stringify(val) : val.toString();
  }

  private handleEvent(event: ScadaEvent) {
    if (event.type === 'AgentStatusChanged') {
      const idx = this.agents.findIndex(a => a.id === event.payload.id);
      if (idx !== -1) {
        this.agents[idx] = { ...this.agents[idx], ...event.payload };
      } else {
        this.agents.push(event.payload);
      }

      // If agent goes offline, mark all its tags as offline
      if (event.payload.status !== 'Online') {
        this.tags.forEach(tag => {
          if (tag.agent_id === event.payload.id) {
            tag.status = 'offline';
          }
        });
      }
    } else if (event.type === 'TagChanged') {
      // Merge live data with existing tag metadata
      const existing = this.tags.get(event.payload.id) || {};
      this.tags.set(event.payload.id, {
        ...existing,
        ...event.payload,
        status: 'online',
        last_heartbeat: new Date()
      });
    }
  }

  isTagOnline(tag: any, agent: AgentData): boolean {
    if (agent.status !== 'Online') return false;

    // Prefer explicitly persisted status from DB/API
    if (tag.status === 'online') {
      return true;
    }

    // Fallback for reactive SSE updates not yet persisted
    if (tag.last_heartbeat) {
      const last = new Date(tag.last_heartbeat).getTime();
      const now = new Date().getTime();
      const interval = (agent.heartbeat_interval_secs || 30) * 1000;
      if (now - last < interval * 2.5) {
        return true;
      }
    }

    return false;
  }

  formatUptime(seconds: number): string {
    const days = Math.floor(seconds / 86400);
    const hours = Math.floor((seconds % 86400) / 3600);
    const mins = Math.floor((seconds % 3600) / 60);
    const secs = seconds % 60;

    let res = '';
    if (days > 0) res += `${days}d `;
    if (hours > 0 || days > 0) res += `${hours}h `;
    res += `${mins}m ${secs}s`;
    return res;
  }
}

