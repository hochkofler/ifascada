import { Component, OnInit } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { ScadaService, TagHistoryEntry } from '../../services/scada.service';

@Component({
  selector: 'app-events-log',
  standalone: true,
  imports: [CommonModule, FormsModule],
  template: `
    <div class="events-container">
      <header class="page-header">
        <h1>Tag Events Log</h1>
          <div class="header-actions">
            <button class="btn-batch-print" *ngIf="selectedIds.size > 0" (click)="printSelected()">
              🖨️ Print Selected ({{ selectedIds.size }})
            </button>
            <div class="select-wrapper">
              <span class="label">Select Tag:</span>
              <select [(ngModel)]="selectedTag" (change)="onTagChange()">
                <option value="">-- Choose a tag --</option>
                <option *ngFor="let tag of tags" [value]="tag.id">{{ tag.id }} ({{ tag.agent_id }})</option>
              </select>
            </div>
            <button class="btn-refresh" (click)="loadEvents()">↻ Refresh</button>
          </div>
      </header>

      <div class="content-card">
        <div class="table-wrapper">
          <table *ngIf="events.length > 0; else noData">
            <thead>
              <tr>
                <th class="check-col">
                  <input type="checkbox" [checked]="isAllSelected()" (change)="toggleAll()" *ngIf="events.length > 0">
                </th>
                <th>Timestamp</th>
                <th>Source Time</th>
                <th>Tag ID</th>
                <th>Value</th>
                <th>Quality</th>
              </tr>
            </thead>
            <tbody>
              <tr *ngFor="let event of events" (click)="toggleSelection(event.id!)" [class.selected-row]="selectedIds.has(event.id!)">
                <td class="check-col" (click)="$event.stopPropagation()">
                  <input type="checkbox" [checked]="selectedIds.has(event.id!)" (change)="toggleSelection(event.id!)">
                </td>
                <td class="dim">{{ parseDate(event.created_at) | date:'yyyy-MM-dd HH:mm:ss.SSS' }}</td>
                <td>{{ parseDate(event.timestamp) | date:'HH:mm:ss.SSS' }}</td>
                <td class="tag-id">{{ selectedTag }}</td>
                <td class="val-col">{{ formatValue(event.value) }}</td>
                <td>
                  <span class="quality-badge" [class.good]="event.quality === 'Good'">
                    {{ event.quality }}
                  </span>
                </td>
              </tr>
            </tbody>
          </table>
          <ng-template #noData>
            <div class="empty-state">
              <p *ngIf="!selectedTag">Please select a tag to view its history.</p>
              <p *ngIf="selectedTag && !loading">No events found for this tag.</p>
              <p *ngIf="loading">Loading data...</p>
            </div>
          </ng-template>
        </div>

        <div class="pagination-footer" *ngIf="selectedTag && (events.length > 0 || offset > 0)">
          <div class="page-info">
            Showing records {{ offset + 1 }} - {{ offset + events.length }}
          </div>
          <div class="page-controls">
            <button [disabled]="offset === 0" (click)="prevPage()" class="btn-nav">Previous</button>
            <button [disabled]="events.length < limit" (click)="nextPage()" class="btn-nav">Next</button>
          </div>
        </div>
      </div>
    </div>
  `,
  styles: [`
    .events-container { padding: 30px; color: #e2e8f0; height: 100%; display: flex; flex-direction: column; gap: 20px; }
    
    .page-header { display: flex; justify-content: space-between; align-items: center; }
    h1 { margin: 0; font-weight: 300; letter-spacing: 1px; color: #fff; }

    .header-actions { display: flex; gap: 20px; align-items: center; }
    .btn-batch-print { background: #3b82f6; border: none; color: white; padding: 8px 16px; border-radius: 8px; cursor: pointer; font-weight: 600; font-size: 0.9em; box-shadow: 0 4px 12px rgba(59, 130, 246, 0.3); }
    .btn-batch-print:hover { background: #2563eb; transform: translateY(-1px); }

    .select-wrapper { display: flex; align-items: center; gap: 10px; }
    .label { color: #94a3b8; font-size: 0.9em; }

    select {
      background: #1e293b;
      border: 1px solid rgba(255,255,255,0.1);
      color: #fff;
      padding: 8px 12px;
      border-radius: 8px;
      outline: none;
      cursor: pointer;
    }

    .btn-refresh {
      background: rgba(59, 130, 246, 0.1);
      border: 1px solid rgba(59, 130, 246, 0.3);
      color: #60a5fa;
      padding: 8px 16px;
      border-radius: 8px;
      cursor: pointer;
      transition: all 0.2s;
    }
    .btn-refresh:hover { background: rgba(59, 130, 246, 0.2); }

    .content-card {
      background: rgba(30, 41, 59, 0.4);
      backdrop-filter: blur(10px);
      border: 1px solid rgba(255, 255, 255, 0.05);
      border-radius: 16px;
      flex: 1;
      display: flex;
      flex-direction: column;
      overflow: hidden;
    }

    .table-wrapper { flex: 1; overflow-y: auto; }
    
    table { width: 100%; border-collapse: collapse; table-layout: fixed; }
    th { 
      position: sticky; top: 0; background: #1e293b; z-index: 10;
      text-align: left; padding: 15px 20px; color: #94a3b8; font-weight: 600; font-size: 0.8em;
      text-transform: uppercase; letter-spacing: 0.05em;
      border-bottom: 2px solid rgba(255,255,255,0.05);
    }
    td { 
      padding: 16px 20px; 
      border-bottom: 1px solid rgba(255,255,255,0.05); 
      font-size: 0.9em; 
      vertical-align: middle;
      line-height: 1.4;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }
    
    tr:hover td { background: rgba(59, 130, 246, 0.05); }
    tr.selected-row td { background: rgba(59, 130, 246, 0.15); }

    .check-col { width: 45px; text-align: center; }
    input[type="checkbox"] { cursor: pointer; accent-color: #3b82f6; width: 16px; height: 16px; }

    .dim { color: #64748b; font-size: 0.85em; }
    .tag-id { color: #cbd5e1; font-family: 'JetBrains Mono', monospace; font-weight: 500; }
    .val-col { font-family: 'JetBrains Mono', monospace; font-weight: 600; color: #60a5fa; font-size: 1em; }

    .quality-badge { 
      font-size: 0.75em; 
      padding: 4px 10px; 
      border-radius: 6px; 
      background: rgba(148, 163, 184, 0.1); 
      color: #94a3b8;
      border: 1px solid rgba(148, 163, 184, 0.2);
    }
    .quality-badge.good { background: rgba(16, 185, 129, 0.1); color: #10b981; border: 1px solid rgba(16, 185, 129, 0.2); }

    .empty-state { height: 100%; display: flex; align-items: center; justify-content: center; color: #64748b; font-style: italic; }

    .pagination-footer {
      padding: 15px 20px;
      background: rgba(15, 23, 42, 0.3);
      border-top: 1px solid rgba(255,255,255,0.05);
      display: flex;
      justify-content: space-between;
      align-items: center;
    }

    .page-info { font-size: 0.85em; color: #94a3b8; }
    .page-controls { display: flex; gap: 10px; }

    .btn-nav {
      background: #1e293b;
      border: 1px solid rgba(255,255,255,0.1);
      color: #e2e8f0;
      padding: 6px 16px;
      border-radius: 6px;
      cursor: pointer;
      font-size: 0.85em;
    }
    .btn-nav:hover:not(:disabled) { background: #334155; }
    .btn-nav:disabled { opacity: 0.3; cursor: not-allowed; }

    /* Scrollbar */
    .table-wrapper::-webkit-scrollbar { width: 8px; }
    .table-wrapper::-webkit-scrollbar-thumb { background: #334155; border-radius: 4px; }
  `]
})
export class EventsLogComponent implements OnInit {
  tags: any[] = [];
  selectedTag: string = '';
  events: TagHistoryEntry[] = [];
  selectedIds: Set<number> = new Set();

  limit = 20;
  offset = 0;
  loading = false;

  constructor(private scada: ScadaService) { }

  ngOnInit() {
    this.scada.getTags().subscribe(data => {
      this.tags = data.sort((a, b) => a.id.localeCompare(b.id));
    });
  }

  onTagChange() {
    this.offset = 0;
    this.loadEvents();
  }

  loadEvents() {
    if (!this.selectedTag) {
      this.events = [];
      return;
    }

    this.loading = true;
    this.scada.getTagHistory(this.selectedTag, this.limit, this.offset).subscribe({
      next: (data) => {
        this.events = data;
        this.loading = false;
        this.selectedIds.clear();
      },
      error: () => {
        this.loading = false;
      }
    });
  }

  nextPage() {
    this.offset += this.limit;
    this.loadEvents();
  }

  prevPage() {
    this.offset = Math.max(0, this.offset - this.limit);
    this.loadEvents();
  }

  toggleSelection(id: number) {
    if (this.selectedIds.has(id)) {
      this.selectedIds.delete(id);
    } else {
      this.selectedIds.add(id);
    }
  }

  toggleAll() {
    if (this.isAllSelected()) {
      this.selectedIds.clear();
    } else {
      this.events.forEach(e => this.selectedIds.add(e.id!));
    }
  }

  isAllSelected() {
    return this.events.length > 0 && this.events.every(e => this.selectedIds.has(e.id!));
  }

  printSelected() {
    if (this.selectedIds.size === 0) return;

    const ids = Array.from(this.selectedIds);
    this.scada.batchPrintEvents(ids).subscribe({
      next: (res) => {
        alert(`Print command sent for ${ids.length} rows!`);
        this.selectedIds.clear();
      },
      error: (e) => alert('Error sending print command: ' + (e.error?.error || e.message))
    });
  }

  parseDate(date: any): string | null {
    if (!date) return null;
    return new Date(date).toISOString(); // the pipe handles it
  }

  formatValue(val: any): string {
    if (typeof val === 'object' && val !== null) {
      if ('value' in val && 'unit' in val) return `${val.value} ${val.unit}`;
      return JSON.stringify(val);
    }
    return String(val);
  }
}
