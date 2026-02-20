import { Component } from '@angular/core';
import { RouterOutlet, RouterLink, RouterLinkActive } from '@angular/router';

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [RouterOutlet, RouterLink, RouterLinkActive],
  template: `
    <div class="app-layout">
      <nav class="sidebar">
        <div class="logo">
          <span class="logo-icon">▲</span>
          <span class="logo-text">IFA SCADA</span>
        </div>
        
        <ul class="nav-links">
          <li>
            <a routerLink="/monitor" routerLinkActive="active">
              <span class="icon">📊</span> Monitoring
            </a>
          </li>
          <li>
            <a routerLink="/events" routerLinkActive="active">
              <span class="icon">📜</span> Tag Events
            </a>
          </li>
          <li>
            <a routerLink="/reports" routerLinkActive="active">
              <span class="icon">🛡️</span> Traceability
            </a>
          </li>
          <li>
            <a routerLink="/trends" routerLinkActive="active">
              <span class="icon">📈</span> Trends
            </a>
          </li>
        </ul>

        <div class="sidebar-footer">
          <div class="system-status">
            <span class="status-dot online"></span> Central Server Online
          </div>
        </div>
      </nav>

      <main class="main-content">
        <router-outlet />
      </main>
    </div>
  `,
  styles: [`
    .app-layout { font-family: 'Segoe UI', Roboto, sans-serif; display: flex; height: 100vh; background: #0f172a; color: #f1f5f9; overflow: hidden; }
    
    .sidebar { width: 260px; background: rgba(30, 41, 59, 0.5); backdrop-filter: blur(15px); border-right: 1px solid rgba(255, 255, 255, 0.05); display: flex; flex-direction: column; padding: 25px; }
    
    .logo { display: flex; align-items: center; gap: 12px; margin-bottom: 40px; }
    .logo-icon { color: #3b82f6; font-size: 1.5em; font-weight: bold; }
    .logo-text { font-size: 1.2em; font-weight: 600; letter-spacing: 1px; color: #fff; }

    .nav-links { list-style: none; padding: 0; margin: 0; flex: 1; }
    .nav-links li { margin-bottom: 15px; }
    .nav-links a { display: flex; align-items: center; gap: 12px; padding: 12px 18px; color: #94a3b8; text-decoration: none; border-radius: 8px; transition: all 0.2s; font-weight: 500; }
    .nav-links a:hover { background: rgba(255,255,255,0.05); color: #fff; }
    .nav-links a.active { background: #3b82f6; color: #fff; box-shadow: 0 4px 12px rgba(59, 130, 246, 0.4); }
    .nav-links .icon { font-size: 1.2em; }

    .main-content { flex: 1; overflow-y: auto; background: radial-gradient(circle at top right, #1e293b, #0f172a); }

    .sidebar-footer { margin-top: auto; padding-top: 20px; border-top: 1px solid rgba(255,255,255,0.05); }
    .system-status { display: flex; align-items: center; gap: 8px; font-size: 0.8em; color: #64748b; }
    .status-dot { width: 8px; height: 8px; border-radius: 50%; }
    .status-dot.online { background: #10b981; box-shadow: 0 0 8px #10b981; }
  `],
})
export class AppComponent {
  title = 'web-dashboard';
}
