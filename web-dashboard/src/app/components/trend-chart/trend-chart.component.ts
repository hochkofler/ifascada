import { Component, OnInit, ViewChild, ElementRef, AfterViewInit, OnDestroy } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { ScadaService, Tag, TagHistoryEntry } from '../../services/scada.service';
import { Chart, registerables } from 'chart.js';

Chart.register(...registerables);

@Component({
    selector: 'app-trend-chart',
    standalone: true,
    imports: [CommonModule, FormsModule],
    templateUrl: './trend-chart.component.html',
    styleUrls: ['./trend-chart.component.css']
})
export class TrendChartComponent implements OnInit, AfterViewInit, OnDestroy {
    @ViewChild('trendChart') chartCanvas!: ElementRef<HTMLCanvasElement>;
    chart?: Chart;

    tags: Tag[] = [];
    selectedTagId: string = '';

    // Controls
    startTime: string = '';
    endTime: string = '';
    limit: number = 1000;

    history: TagHistoryEntry[] = [];
    isLoading = false;
    errorMsg = '';

    constructor(private scada: ScadaService) { }

    ngOnInit() {
        this.loadTags();

        // Default time range: last 1 hour
        const now = new Date();
        const oneHourAgo = new Date(now.getTime() - 60 * 60 * 1000);

        this.endTime = this.toDateTimeLocal(now);
        this.startTime = this.toDateTimeLocal(oneHourAgo);
    }

    ngAfterViewInit() {
        this.initChart();
    }

    ngOnDestroy() {
        this.chart?.destroy();
    }

    loadTags() {
        this.scada.getTags().subscribe({
            next: (tags) => {
                this.tags = tags;
                if (tags.length > 0) {
                    this.selectedTagId = tags[0].id;
                    this.fetchHistory();
                }
            },
            error: (err) => this.errorMsg = 'Failed to load tags'
        });
    }

    onTagChange() {
        this.fetchHistory();
    }

    refresh() {
        this.fetchHistory();
    }

    fetchHistory() {
        if (!this.selectedTagId) return;

        this.isLoading = true;
        this.errorMsg = '';

        // Convert local inputs to ISO strings for API
        const startIso = new Date(this.startTime).toISOString();
        const endIso = new Date(this.endTime).toISOString();

        this.scada.getTagHistory(this.selectedTagId, this.limit, 0, startIso, endIso)
            .subscribe({
                next: (data) => {
                    this.history = data; // API returns ordered by desc or asc? Backend default was desc? 
                    // Backend with start/end sorts ASC. 
                    // Wait, api.rs: "ORDER BY timestamp ASC" for range queries.
                    // "ORDER BY timestamp DESC" for no-range (default).
                    // So if we send start/end, we get ASC. 

                    this.updateChart();
                    this.isLoading = false;
                },
                error: (err) => {
                    this.errorMsg = 'Failed to load history';
                    this.isLoading = false;
                }
            });
    }

    initChart() {
        const ctx = this.chartCanvas.nativeElement.getContext('2d');
        if (!ctx) return;

        this.chart = new Chart(ctx, {
            type: 'line',
            data: {
                labels: [],
                datasets: [{
                    label: 'Value',
                    data: [],
                    borderColor: '#3b82f6',
                    borderWidth: 2,
                    pointRadius: 2,
                    pointHoverRadius: 5,
                    tension: 0.1,
                    fill: true,
                    backgroundColor: 'rgba(59, 130, 246, 0.1)'
                }]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                plugins: {
                    legend: { display: true },
                    tooltip: {
                        mode: 'index',
                        intersect: false
                    }
                },
                interaction: {
                    mode: 'nearest',
                    axis: 'x',
                    intersect: false
                },
                scales: {
                    x: {
                        display: true,
                        title: { display: true, text: 'Time' },
                        ticks: {
                            maxTicksLimit: 10,
                            color: '#94a3b8'
                        },
                        grid: { color: 'rgba(255,255,255,0.05)' }
                    },
                    y: {
                        display: true,
                        title: { display: true, text: 'Value' },
                        grid: { color: 'rgba(255,255,255,0.05)' },
                        ticks: { color: '#94a3b8' }
                    }
                }
            }
        });
    }

    updateChart() {
        if (!this.chart) return;

        // Data is ASC from backend if range used.
        // If not range, DESC. But we always set start/end in ngOnInit.
        // However, if fetchHistory is called, we pass them.

        // We assume data matches the order we want (ASC).

        const labels = this.history.map(h => {
            const d = new Date(h.created_at || h.timestamp);
            // Backend returns `timestamp` (OffsetDateTime). `created_at` might be null.
            // Use `timestamp` preferably.
            return d.toLocaleString();
        });

        const dataPoints = this.history.map(h => this.extractNumericValue(h.value));

        this.chart.data.labels = labels;
        this.chart.data.datasets[0].label = this.selectedTagId;
        this.chart.data.datasets[0].data = dataPoints;
        this.chart.update();
    }

    private extractNumericValue(val: any): number {
        if (typeof val === 'number') return val;
        if (typeof val === 'string') {
            const parsed = parseFloat(val);
            return isNaN(parsed) ? 0 : parsed;
        }
        if (typeof val === 'object' && val !== null) {
            if ('value' in val) return this.extractNumericValue(val.value);
        }
        return 0;
    }

    private toDateTimeLocal(date: Date): string {
        // Format: YYYY-MM-DDTHH:mm
        const pad = (n: number) => n < 10 ? '0' + n : n;
        return date.getFullYear() +
            '-' + pad(date.getMonth() + 1) +
            '-' + pad(date.getDate()) +
            'T' + pad(date.getHours()) +
            ':' + pad(date.getMinutes());
    }
}
