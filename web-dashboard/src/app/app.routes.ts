import { Routes } from '@angular/router';
import { MonitorComponent } from './components/monitor/monitor.component';
import { ReportsComponent } from './components/reports/reports.component';
import { EventsLogComponent } from './components/events-log/events-log.component';

export const routes: Routes = [
    { path: '', redirectTo: 'monitor', pathMatch: 'full' },
    { path: 'monitor', component: MonitorComponent },
    { path: 'reports', component: ReportsComponent },
    { path: 'events', component: EventsLogComponent },
];
