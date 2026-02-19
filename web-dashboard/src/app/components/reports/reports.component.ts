import { Component, OnInit } from '@angular/core';
import { CommonModule } from '@angular/common';
import { ScadaService, ReportSummary, ReportDetail } from '../../services/scada.service';

@Component({
  selector: 'app-reports',
  standalone: true,
  imports: [CommonModule],
  template: `
    <div class="reports-container">
      <header>
        <h1>Traceability Reports</h1>
        <div class="header-actions" *ngIf="selectedIds.size > 0">
          <button class="btn-batch-reprint" (click)="reprintSelected()">
            🖨️ Reprint Selected ({{ selectedIds.size }})
          </button>
        </div>
      </header>

      <div class="content-split">
        <section class="reports-table">
          <table>
            <thead>
              <tr>
                <th class="check-col">
                  <input type="checkbox" [checked]="isAllSelected()" (change)="toggleAll()">
                </th>
                <th>Date</th>
                <th>Report ID</th>
                <th>Agent</th>
                <th>Total</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              <tr *ngFor="let report of reports" (click)="selectReport(report)" [class.active]="selectedReport?.id === report.id">
                <td class="check-col" (click)="$event.stopPropagation()">
                  <input type="checkbox" [checked]="selectedIds.has(report.id)" (change)="toggleSelection(report.id)">
                </td>
                <td>{{ parseDate(report.created_at) | date:'yyyy-MM-dd HH:mm' }}</td>
                <td>{{ report.report_id }}</td>
                <td>{{ report.agent_id }}</td>
                <td>{{ report.total_value | number:'1.2-2' }}</td>
                <td>
                  <button class="btn-detail" (click)="selectReport(report); $event.stopPropagation()">View</button>
                </td>
              </tr>
            </tbody>
          </table>
          <div *ngIf="reports.length === 0" class="empty-state">No reports found</div>
          
          <div class="pagination-controls" *ngIf="reports.length > 0 || offset > 0">
            <button [disabled]="offset === 0" (click)="prevPage()">Previous</button>
            <span>Page {{ (offset / limit) + 1 }}</span>
            <button [disabled]="reports.length < limit" (click)="nextPage()">Next</button>
          </div>
        </section>

        <aside class="detail-panel" *ngIf="selectedDetail">
          <div class="detail-header">
            <h2>{{ selectedDetail.report_id }}</h2>
            <button class="btn-reprint" (click)="reprint(selectedDetail.id)">🖨️ Reprint</button>
          </div>
          
          <div class="detail-meta">
            <p><strong>Agent:</strong> {{ selectedDetail.agent_id }}</p>
            <p><strong>Started:</strong> {{ parseDate(selectedDetail.start_time) | date:'HH:mm:ss' }}</p>
            <p><strong>Ended:</strong> {{ parseDate(selectedDetail.end_time) | date:'HH:mm:ss' }}</p>
            <p><strong>Total Value:</strong> {{ selectedDetail.total_value }}</p>
          </div>

          <h3>Readings</h3>
          <ul class="reading-list">
            <li *ngFor="let item of selectedDetail.items; let i = index">
              <span class="idx">{{ i + 1 }}.</span>
              <span class="val">{{ formatItemValue(item.value) }}</span>
              <span class="ts">{{ parseDate(item.timestamp) | date:'HH:mm:ss' }}</span>
            </li>
          </ul>
        </aside>
      </div>
    </div>
  `,
  styles: [`
    header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 20px; }
    h1 { margin: 0; font-weight: 300; }
    
    .btn-batch-reprint { background: #3b82f6; border: none; color: white; padding: 8px 16px; border-radius: 6px; cursor: pointer; font-weight: bold; animation: fadeIn 0.2s; }
    .btn-batch-reprint:hover { background: #2563eb; }
    @keyframes fadeIn { from { opacity: 0; } to { opacity: 1; } }

    .check-col { width: 40px; text-align: center; }
    input[type="checkbox"] { cursor: pointer; accent-color: #3b82f6; }

    .content-split { display: grid; grid-template-columns: 1fr 350px; gap: 20px; flex: 1; min-height: 0; }
    
    .reports-table { 
      background: rgba(30, 41, 59, 0.4); 
      border-radius: 12px; 
      padding: 10px; 
      overflow-y: auto; 
    }

    table { width: 100%; border-collapse: collapse; text-align: left; }
    th { padding: 12px; border-bottom: 2px solid rgba(255,255,255,0.1); color: #94a3b8; font-size: 0.85em; text-transform: uppercase; }
    td { padding: 12px; border-bottom: 1px solid rgba(255,255,255,0.05); font-size: 0.9em; cursor: pointer; }
    tr:hover { background: rgba(255,255,255,0.05); }
    tr.active { background: rgba(59, 130, 246, 0.2); }

    .btn-detail { background: #334155; border: none; color: white; padding: 4px 12px; border-radius: 4px; cursor: pointer; }
    .btn-detail:hover { background: #475569; }

    .detail-panel { 
      background: rgba(15, 23, 42, 0.9); 
      border: 1px solid rgba(255,255,255,0.1); 
      border-radius: 12px; 
      padding: 20px;
      overflow-y: auto;
    }

    .detail-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 20px; }
    .btn-reprint { background: #10b981; border: none; color: white; padding: 8px 16px; border-radius: 6px; cursor: pointer; font-weight: bold; }
    .btn-reprint:hover { background: #059669; transform: scale(1.05); }

    .detail-meta p { margin: 5px 0; font-size: 0.9em; color: #cbd5e1; }
    .reading-list { list-style: none; padding: 0; margin-top: 15px; }
    .reading-list li { 
      display: flex; gap: 10px; padding: 8px; border-bottom: 1px solid rgba(255,255,255,0.05); font-family: 'JetBrains Mono', monospace; font-size: 0.85em; 
    }
    .idx { color: #64748b; width: 25px; }
    .val { flex: 1; color: #60a5fa; font-weight: bold; }
    .ts { color: #475569; }

    .empty-state { text-align: center; padding: 50px; color: #475569; font-style: italic; }

    .pagination-controls {
      display: flex;
      justify-content: center;
      align-items: center;
      gap: 20px;
      margin-top: 20px;
      padding: 10px;
      border-top: 1px solid rgba(255,255,255,0.05);
    }
    .pagination-controls button {
      background: #334155;
      border: none;
      color: white;
      padding: 6px 16px;
      border-radius: 4px;
      cursor: pointer;
    }
    .pagination-controls button:disabled {
      opacity: 0.3;
      cursor: not-allowed;
    }
    .pagination-controls span {
      font-size: 0.9em;
      color: #94a3b8;
    }
  `]
})
export class ReportsComponent implements OnInit {
  reports: ReportSummary[] = [];
  selectedReport: ReportSummary | null = null;
  selectedDetail: ReportDetail | null = null;
  selectedIds: Set<string> = new Set();
  limit: number = 20;
  offset: number = 0;

  constructor(private scada: ScadaService) { }

  ngOnInit() {
    this.refresh();
  }

  refresh() {
    this.scada.getReports(this.limit, this.offset).subscribe(data => this.reports = data);
  }

  nextPage() {
    this.offset += this.limit;
    this.refresh();
    this.selectedIds.clear();
  }

  prevPage() {
    if (this.offset >= this.limit) {
      this.offset -= this.limit;
      this.refresh();
      this.selectedIds.clear();
    }
  }

  selectReport(report: ReportSummary) {
    this.selectedReport = report;
    this.scada.getReportDetails(report.id).subscribe(detail => {
      this.selectedDetail = detail;
    });
  }

  toggleSelection(id: string) {
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
      this.reports.forEach(r => this.selectedIds.add(r.id));
    }
  }

  isAllSelected() {
    return this.reports.length > 0 && this.selectedIds.size === this.reports.length;
  }

  reprint(id: string) {
    this.scada.reprintReport(id).subscribe(() => {
      alert('Reprint command sent to agent!');
    });
  }

  reprintSelected() {
    if (this.selectedIds.size === 0) return;

    const count = this.selectedIds.size;
    Array.from(this.selectedIds).forEach(id => {
      this.scada.reprintReport(id).subscribe();
    });

    alert(`Reprint commands for ${count} reports sent!`);
    this.selectedIds.clear();
  }
  formatItemValue(val: any): string {
    if (val === null || val === undefined) return '---';
    if (typeof val === 'object') {
      if ('value' in val && 'unit' in val) {
        return `${val.value} ${val.unit}`;
      }
      return JSON.stringify(val);
    }
    return String(val);
  }

  parseDate(date: any): any {
    if (!date) return null;
    if (typeof date === 'string') {
      let s = date.trim();
      s = s.replace(/^(\d{4}-\d{2}-\d{2})\s+/, '$1T');
      s = s.replace(/\s+/g, '');
      s = s.replace(/([-+]\d{2}:\d{2}):\d{2}$/, '$1');
      return s;
    }
    return date;
  }
}
