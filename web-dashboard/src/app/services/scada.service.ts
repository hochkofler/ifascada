import { Injectable } from '@angular/core';
import { HttpClient } from '@angular/common/http';
import { Observable } from 'rxjs';

export interface AgentData {
    id: string;
    status: 'Online' | 'Offline' | 'Unknown';
    last_seen: string;
    is_registered: boolean;
    heartbeat_interval_secs?: number;
    missed_threshold?: number;
    metrics?: {
        uptime: number;
        tags: number;
        tag_ids?: string[];
        ts: number;
    };
}

export interface Tag {
    id: string;
    agent_id: string;
    value: any;
    quality: string;
    status: string;
    timestamp: string;
}

export interface ReportSummary {
    id: string;
    report_id: string;
    agent_id: string;
    start_time: string;
    end_time: string;
    total_value: number;
    created_at: string;
}

export interface ReportDetail extends ReportSummary {
    items: Array<{
        value: any;
        timestamp: string;
    }>;
}

export interface TagHistoryEntry {
    id?: number;
    value: any;
    quality: string;
    timestamp: string;
    created_at: string;
}

@Injectable({
    providedIn: 'root'
})
export class ScadaService {
    private baseUrl = `http://${window.location.hostname}:3000/api`;

    constructor(private http: HttpClient) { }

    getAgents(): Observable<AgentData[]> {
        return this.http.get<AgentData[]>(`${this.baseUrl}/agents`);
    }

    getReports(limit: number = 20, offset: number = 0): Observable<ReportSummary[]> {
        return this.http.get<ReportSummary[]>(`${this.baseUrl}/reports?limit=${limit}&offset=${offset}`);
    }

    getTags(): Observable<Tag[]> {
        return this.http.get<Tag[]>(`${this.baseUrl}/tags`);
    }

    getReportDetails(id: string): Observable<ReportDetail> {
        return this.http.get<ReportDetail>(`${this.baseUrl}/reports/${id}`);
    }

    reprintReport(id: string): Observable<any> {
        return this.http.post(`${this.baseUrl}/reports/${id}/reprint`, {});
    }

    getTagHistory(id: string, limit: number = 30, offset: number = 0, start?: string, end?: string, order?: 'asc' | 'desc'): Observable<TagHistoryEntry[]> {
        let params = `limit=${limit}&offset=${offset}`;
        if (start) params += `&start=${start}`;
        if (end) params += `&end=${end}`;
        if (order) params += `&order=${order}`;
        return this.http.get<TagHistoryEntry[]>(`${this.baseUrl}/tags/${id}/history?${params}`);
    }

    batchPrintEvents(eventIds: number[]): Observable<any> {
        return this.http.post(`${this.baseUrl}/tags/batch-print`, { event_ids: eventIds });
    }
}
