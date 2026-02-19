import { Injectable, NgZone } from '@angular/core';
import { Observable, Subject } from 'rxjs';

export interface TagChangedEvent {
    type: 'TagChanged';
    payload: {
        id: string;
        agent_id: string;
        value: any;
        quality: string;
        timestamp: string;
        received_at?: string;
    };
}

export interface AgentStatusEvent {
    type: 'AgentStatusChanged';
    payload: {
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
    };
}

export interface ReportCompletedEvent {
    type: 'ReportCompleted';
    payload: {
        report_id: string;
        agent_id: string;
        items: any[];
        timestamp: string;
    };
}

export type ScadaEvent = TagChangedEvent | AgentStatusEvent | ReportCompletedEvent;

@Injectable({
    providedIn: 'root'
})
export class SseService {
    private url = `http://${window.location.hostname}:3000/api/events`;
    private eventSubject = new Subject<ScadaEvent>();

    constructor(private zone: NgZone) {
        this.connect();
    }

    private connect() {
        const eventSource = new EventSource(this.url);

        eventSource.onmessage = (event) => {
            this.zone.run(() => {
                try {
                    const data: ScadaEvent = JSON.parse(event.data);
                    this.eventSubject.next(data);
                } catch (e) {
                    console.error('Error parsing SSE event', e);
                }
            });
        };

        eventSource.onerror = (error) => {
            console.error('SSE Error', error);
            // EventSource automatically reconnects, but we can log it.
        };
    }

    getEvents(): Observable<ScadaEvent> {
        return this.eventSubject.asObservable();
    }
}
