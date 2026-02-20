import { Component, Input, OnInit, OnChanges, SimpleChanges, OnDestroy, ViewChild, ElementRef, AfterViewInit } from '@angular/core';
import { CommonModule } from '@angular/common';
import { ScadaService, TagHistoryEntry } from '../../services/scada.service';
import { SseService, ScadaEvent } from '../../services/sse.service';
import { Subscription } from 'rxjs';
import { Chart, registerables } from 'chart.js';

Chart.register(...registerables);

@Component({
  selector: 'app-tag-history',
  standalone: true,
  imports: [CommonModule],
  template: `
    <div class="history-card">
      <div class="history-header">
        <h3>Trend live: {{ tagId }}</h3>
        <button class="btn-close" (click)="onClose()">×</button>
      </div>

      <div class="chart-container">
        <canvas #historyChart></canvas>
      </div>

      <div class="table-container">
        <table>
          <thead>
            <tr>
              <th>Timestamp</th>
              <th>Value</th>
              <th>Quality</th>
            </tr>
          </thead>
          <tbody>
            <tr *ngFor="let entry of history">
              <td>{{ parseDate(entry.timestamp) | date:'HH:mm:ss.SSS' }}</td>
              <td class="val-col">{{ formatValue(entry.value) }}</td>
              <td>
                <span class="quality-badge" [class.good]="entry.quality === 'Good'">
                  {{ entry.quality }}
                </span>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  `,
  styles: [`
    .history-card {
      background: rgba(15, 23, 42, 0.95);
      border: 1px solid rgba(255, 255, 255, 0.1);
      border-radius: 16px;
      padding: 20px;
      display: flex;
      flex-direction: column;
      gap: 20px;
      box-shadow: 0 20px 50px rgba(0,0,0,0.5);
      animation: slideIn 0.3s ease-out;
    }

    @keyframes slideIn {
      from { transform: translateX(100%); opacity: 0; }
      to { transform: translateX(0); opacity: 1; }
    }

    .history-header { display: flex; justify-content: space-between; align-items: center; }
    h3 { margin: 0; font-weight: 500; color: #60a5fa; }
    .btn-close { background: transparent; border: none; color: #64748b; font-size: 1.5rem; cursor: pointer; }
    .btn-close:hover { color: #fff; }

    .chart-container { height: 250px; position: relative; }

    .table-container { 
      max-height: 200px; 
      overflow-y: auto; 
      border-top: 1px solid rgba(255,255,255,0.05);
      padding-top: 10px;
    }

    table { width: 100%; border-collapse: collapse; font-size: 0.85em; }
    th { text-align: left; padding: 8px; color: #94a3b8; border-bottom: 1px solid rgba(255,255,255,0.1); }
    td { padding: 8px; border-bottom: 1px solid rgba(255,255,255,0.02); }
    .val-col { font-family: 'JetBrains Mono', monospace; font-weight: bold; color: #60a5fa; }

    .quality-badge { font-size: 0.75em; padding: 2px 6px; border-radius: 4px; background: #334155; color: #94a3b8; }
    .quality-badge.good { color: #10b981; border: 1px solid rgba(16, 185, 129, 0.2); }

    /* Scrollbar */
    .table-container::-webkit-scrollbar { width: 4px; }
    .table-container::-webkit-scrollbar-thumb { background: #334155; border-radius: 2px; }
  `]
})
export class TagHistoryComponent implements OnInit, OnChanges, OnDestroy, AfterViewInit {
  @Input() tagId!: string;
  @Input() agentId!: string;
  @Input() closeCallback?: () => void;

  @ViewChild('historyChart') chartCanvas!: ElementRef<HTMLCanvasElement>;
  chart?: Chart;

  history: TagHistoryEntry[] = [];
  private sseSub?: Subscription;

  constructor(private scada: ScadaService, private sse: SseService) { }

  ngOnInit() {
    this.loadHistory();
    this.subscribeToRealtime();
  }

  ngOnChanges(changes: SimpleChanges) {
    if (changes['tagId'] && !changes['tagId'].isFirstChange()) {
      this.loadHistory();
    }
  }

  ngAfterViewInit() {
    this.initChart();
    if (this.history.length > 0) {
      this.updateChart();
    }
  }

  ngOnDestroy() {
    this.sseSub?.unsubscribe();
    this.chart?.destroy();
  }

  private loadHistory() {
    this.scada.getTagHistory(this.tagId, 30).subscribe(data => {
      this.history = data;
      if (this.chart) {
        this.updateChart();
      }
    });
  }

  private subscribeToRealtime() {
    this.sseSub = this.sse.getEvents().subscribe(event => {
      if (event.type === 'TagChanged' && event.payload.id === this.tagId) {
        const entry: TagHistoryEntry = {
          value: event.payload.value,
          quality: event.payload.quality,
          timestamp: event.payload.timestamp,
          created_at: event.payload.received_at || new Date().toISOString()
        };
        this.history = [entry, ...this.history].slice(0, 100);
        this.updateChart();
      }
    });
  }

  private initChart() {
    const ctx = this.chartCanvas.nativeElement.getContext('2d');
    if (!ctx) return;

    this.chart = new Chart(ctx, {
      type: 'line',
      data: {
        labels: [],
        datasets: [{
          label: this.tagId,
          data: [],
          borderColor: '#3b82f6',
          borderWidth: 2,
          pointRadius: 0,
          tension: 0.3,
          fill: true,
          backgroundColor: 'rgba(59, 130, 246, 0.1)'
        }]
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        plugins: { legend: { display: false } },
        scales: {
          x: { display: false },
          y: {
            grid: { color: 'rgba(255,255,255,0.05)' },
            ticks: { color: '#64748b', font: { size: 10 } }
          }
        }
      }
    });
  }

  private updateChart() {
    if (!this.chart) return;

    // Use a copy to avoid reversing original array if needed, 
    // but here we want chronological for the chart (left to right)
    const displayData = [...this.history].reverse();

    this.chart.data.labels = displayData.map(h => {
      const parsed = this.parseDate(h.timestamp);
      return parsed ? new Date(parsed).toLocaleTimeString() : 'Unknown';
    });
    this.chart.data.datasets[0].data = displayData.map(h => this.extractNumericValue(h.value));
    this.chart.update('none'); // Update without animation for performance
  }

  private extractNumericValue(val: any): number {
    if (typeof val === 'number') return val;
    if (typeof val === 'object' && val !== null) {
      // Logic similar to get_primary_value in Rust.
      // We don't have easy access to tag metadata here unless we pass it, 
      // but we can look for "value" or the first numeric field.
      if ('value' in val && typeof val.value === 'number') return val.value;

      for (const v of Object.values(val)) {
        if (typeof v === 'number') return v;
      }
    }
    return 0;
  }

  formatValue(val: any): string {
    if (typeof val === 'object' && val !== null) {
      if ('value' in val && 'unit' in val) return `${val.value} ${val.unit}`;

      const parts: string[] = [];
      for (const [k, v] of Object.entries(val)) {
        parts.push(`${k}: ${v}`);
      }
      return parts.join(', ');
    }
    return String(val);
  }

  parseDate(date: any): string | null {
    if (!date) return null;
    return new Date(date).toISOString();
  }

  onClose() {
    if (this.closeCallback) this.closeCallback();
  }
}
