import React, { useState, useRef, useEffect } from "react";
import { createRoot } from "react-dom/client";
import { GoogleGenAI } from "@google/genai";
import { Activity, Clock, Search, Lock, Cpu, Zap, GitBranch, Github, Menu, X } from 'lucide-react';

// --- Components ---

const Logo = () => {
  return (
    <div className="flex items-center gap-3 group select-none cursor-pointer">
      <div className="relative w-10 h-10 rounded-xl bg-gradient-to-tr from-blue-600 to-cyan-500 shadow-lg shadow-blue-500/20 flex items-center justify-center overflow-hidden transition-transform group-hover:scale-105 duration-300">
        <div className="absolute inset-0 bg-white/20 opacity-0 group-hover:opacity-100 transition-opacity" />
        <div className="absolute -inset-1 bg-gradient-to-r from-transparent via-white/30 to-transparent -translate-x-full group-hover:animate-[shimmer_1.5s_infinite]" />
        <Activity className="h-5 w-5 text-white relative z-10" />
      </div>
      <span className="text-slate-400 group-hover:text-blue-600 transition-colors duration-300">|</span>

      <div className="flex flex-col">
        <h1 className="text-lg font-bold tracking-tight text-slate-900 group-hover:text-blue-600 transition-colors duration-300">
          ScreenSearch
        </h1>
        <p className="text-[10px] uppercase tracking-wider font-semibold text-slate-500 group-hover:text-blue-600/70 transition-colors">
          Search History
        </p>
      </div>
    </div>
  );
}

const Header = () => {
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);

  return (
    <>
      <nav className="fixed w-full z-50 bg-white/90 backdrop-blur-xl border-b border-slate-200">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
          <div className="flex items-center justify-between h-16">
            <Logo />

            {/* Desktop Navigation */}
            <div className="hidden lg:block">
              <div className="ml-10 flex items-baseline space-x-8">
                <a href="#features" className="hover:text-blue-600 text-slate-600 px-3 py-2 rounded-md text-sm font-medium transition-colors">Features</a>
                <a href="#how-it-works" className="hover:text-blue-600 text-slate-600 px-3 py-2 rounded-md text-sm font-medium transition-colors">How it Works</a>
                <a href="#demo" className="hover:text-blue-600 text-slate-600 px-3 py-2 rounded-md text-sm font-medium transition-colors">Live Demo</a>
                <a href="https://github.com/nicolasestrem/screensearch" target="_blank" rel="noreferrer" className="text-slate-500 hover:text-slate-900 transition-colors">
                  <Github className="w-5 h-5" />
                </a>
              </div>
            </div>

            {/* Right Side Buttons */}
            <div className="flex items-center gap-2">
              <a href="https://github.com/nicolasestrem/screensearch/releases" className="bg-slate-900 text-white hover:bg-blue-600 hover:shadow-blue-500/30 px-3 sm:px-5 py-2 sm:py-2.5 rounded-lg text-xs sm:text-sm font-semibold transition-all shadow-md transform hover:-translate-y-0.5 whitespace-nowrap">
                <span className="sm:hidden">Download</span><span className="hidden sm:inline">Download Beta</span>
              </a>

              {/* Mobile Menu Button */}
              <button
                onClick={() => setMobileMenuOpen(!mobileMenuOpen)}
                className="lg:hidden p-2 rounded-lg text-slate-600 hover:text-blue-600 hover:bg-slate-100 transition-colors ml-2"
                aria-label="Toggle menu"
              >
                {mobileMenuOpen ? <X className="w-6 h-6" /> : <Menu className="w-6 h-6" />}
              </button>
            </div>
          </div>
        </div>
      </nav>

      {/* Mobile Menu Drawer */}
      <div
        className={`fixed inset-0 z-40 lg:hidden transition-opacity duration-300 ${mobileMenuOpen ? 'opacity-100 pointer-events-auto' : 'opacity-0 pointer-events-none'
          }`}
      >
        {/* Backdrop */}
        <div
          className="absolute inset-0 bg-slate-900/50 backdrop-blur-sm"
          onClick={() => setMobileMenuOpen(false)}
        />

        {/* Menu Panel */}
        <div
          className={`absolute top-16 right-0 left-0 bg-white border-b border-slate-200 shadow-xl transition-transform duration-300 ${mobileMenuOpen ? 'translate-y-0' : '-translate-y-full'
            }`}
        >
          <div className="max-w-7xl mx-auto px-4 py-6 space-y-1">
            <a
              href="#features"
              onClick={() => setMobileMenuOpen(false)}
              className="block px-4 py-3 rounded-lg text-slate-700 hover:bg-blue-50 hover:text-blue-600 font-medium transition-colors"
            >
              Features
            </a>
            <a
              href="#how-it-works"
              onClick={() => setMobileMenuOpen(false)}
              className="block px-4 py-3 rounded-lg text-slate-700 hover:bg-blue-50 hover:text-blue-600 font-medium transition-colors"
            >
              How it Works
            </a>
            <a
              href="#demo"
              onClick={() => setMobileMenuOpen(false)}
              className="block px-4 py-3 rounded-lg text-slate-700 hover:bg-blue-50 hover:text-blue-600 font-medium transition-colors"
            >
              Live Demo
            </a>
            <a
              href="https://github.com/nicolasestrem/screensearch"
              target="_blank"
              rel="noreferrer"
              className="block px-4 py-3 rounded-lg text-slate-700 hover:bg-blue-50 hover:text-blue-600 font-medium transition-colors flex items-center gap-2"
            >
              <Github className="w-5 h-5" />
              <span>View on GitHub</span>
            </a>
          </div>
        </div>
      </div>
    </>
  );
};

const Hero = () => (
  <div className="relative pt-32 pb-20 sm:pt-40 sm:pb-24 overflow-hidden bg-white bg-grid">
    <div className="relative max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 text-center z-10">
      <div className="inline-flex items-center gap-2 px-3 py-1 rounded-full bg-blue-50 border border-blue-100 text-blue-600 text-xs font-bold uppercase tracking-wide mb-8 shadow-sm">
        <span className="relative flex h-2 w-2">
          <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-blue-400 opacity-75"></span>
          <span className="relative inline-flex rounded-full h-2 w-2 bg-blue-500"></span>
        </span>
        ScreenSearch v0.1.0 - Pre-release
      </div>
      <h1 className="text-5xl sm:text-7xl font-extrabold text-slate-900 tracking-tight mb-6">
        Your Screen History, <br />
        <span className="text-transparent bg-clip-text bg-gradient-to-r from-blue-600 to-cyan-500">Fully Searchable</span>
      </h1>
      <p className="mt-6 text-xl text-slate-600 max-w-2xl mx-auto mb-10 font-normal leading-relaxed">
        The privacy-first alternative to Windows Recall. Search everything you've ever seen on your screen with powerful OCR.
        <strong className="text-slate-700"> 100% local. Zero cloud uploads. Completely open source.</strong>
      </p>
      <div className="flex flex-col sm:flex-row justify-center gap-4">
        <a href="https://github.com/nicolasestrem/screensearch/releases" className="px-8 py-4 rounded-xl bg-blue-600 hover:bg-blue-700 text-white font-bold text-lg transition-all shadow-xl shadow-blue-600/20 flex items-center justify-center gap-3 transform hover:-translate-y-1">
          <i className="fa-brands fa-windows text-2xl"></i> Download for Windows
        </a>
        <a href="https://github.com/nicolasestrem/screensearch" className="px-8 py-4 rounded-xl bg-white hover:bg-slate-50 text-slate-900 font-semibold text-lg transition-all border border-slate-200 hover:border-blue-200 shadow-sm hover:shadow-md flex items-center justify-center gap-3 group">
          <Github className="w-5 h-5 text-slate-600 group-hover:text-blue-600 transition-colors" />
          View Source
        </a>
      </div>
      <div className="mt-8 flex flex-col items-center justify-center gap-2 text-sm text-slate-500 font-medium">
        <div className="flex items-center gap-2 text-slate-600">
          <i className="fa-solid fa-circle-check text-emerald-500"></i>
          <span>Windows 10/11 Compatible</span>
        </div>
        <span className="text-slate-400 text-xs">(MacOS and Linux support coming soon)</span>
      </div>
    </div>
  </div>
);

const WhyScreenSearch = () => (
  <div className="py-16 bg-slate-50/50 border-y border-slate-100">
    <div className="max-w-5xl mx-auto px-4 sm:px-6 lg:px-8">
      <div className="text-center mb-12">
        <h2 className="text-3xl font-bold text-slate-900 sm:text-4xl mb-4">Why ScreenSearch?</h2>
        <p className="text-lg text-slate-600 max-w-3xl mx-auto">
          In 2024, Microsoft announced <strong>Windows Recall</strong>—a feature that would continuously capture screenshots of everything you do.
          While the concept was promising, the execution raised serious concerns.
        </p>
      </div>

      <div className="grid md:grid-cols-2 gap-6 mb-12">
        <div className="p-6 bg-red-50 border border-red-200 rounded-xl">
          <h3 className="font-bold text-red-900 mb-3 flex items-center gap-2">
            <i className="fa-solid fa-triangle-exclamation text-red-600"></i>
            Windows Recall Problems
          </h3>
          <ul className="space-y-2 text-sm text-red-800">
            <li className="flex items-start gap-2">
              <span className="text-red-600 mt-0.5">❌</span>
              <span><strong>Security vulnerabilities:</strong> Early versions stored data unencrypted</span>
            </li>
            <li className="flex items-start gap-2">
              <span className="text-red-600 mt-0.5">❌</span>
              <span><strong>Privacy invasion:</strong> Constant surveillance without clear opt-out</span>
            </li>
            <li className="flex items-start gap-2">
              <span className="text-red-600 mt-0.5">❌</span>
              <span><strong>Trust issues:</strong> Closed-source meant no way to verify privacy claims</span>
            </li>
            <li className="flex items-start gap-2">
              <span className="text-red-600 mt-0.5">❌</span>
              <span><strong>Limited hardware:</strong> Required expensive Copilot+ PCs</span>
            </li>
          </ul>
        </div>

        <div className="p-6 bg-emerald-50 border border-emerald-200 rounded-xl">
          <h3 className="font-bold text-emerald-900 mb-3 flex items-center gap-2">
            <i className="fa-solid fa-shield-check text-emerald-600"></i>
            ScreenSearch Solution
          </h3>
          <ul className="space-y-2 text-sm text-emerald-800">
            <li className="flex items-start gap-2">
              <span className="text-emerald-600 mt-0.5">✓</span>
              <span><strong>Open source & auditable:</strong> MIT licensed, inspect every line of code</span>
            </li>
            <li className="flex items-start gap-2">
              <span className="text-emerald-600 mt-0.5">✓</span>
              <span><strong>Local-first architecture:</strong> Your data stays on your device, period</span>
            </li>
            <li className="flex items-start gap-2">
              <span className="text-emerald-600 mt-0.5">✓</span>
              <span><strong>Zero telemetry:</strong> We literally cannot access your data</span>
            </li>
            <li className="flex items-start gap-2">
              <span className="text-emerald-600 mt-0.5">✓</span>
              <span><strong>Works on any PC:</strong> Windows 10/11, no special hardware required</span>
            </li>
          </ul>
        </div>
      </div>

      <div className="bg-white rounded-2xl p-8 border border-slate-200 shadow-sm">
        <h3 className="text-xl font-bold text-slate-900 mb-4 text-center">Our Privacy Principles</h3>
        <div className="grid md:grid-cols-4 gap-6">
          <div className="text-center">
            <div className="w-12 h-12 rounded-full bg-blue-100 flex items-center justify-center mx-auto mb-3">
              <i className="fa-brands fa-github text-blue-600 text-xl"></i>
            </div>
            <h4 className="font-bold text-slate-900 text-sm mb-1">Open Source</h4>
            <p className="text-xs text-slate-600">Every line of code on GitHub. Fully auditable.</p>
          </div>
          <div className="text-center">
            <div className="w-12 h-12 rounded-full bg-green-100 flex items-center justify-center mx-auto mb-3">
              <Lock className="w-6 h-6 text-green-600" />
            </div>
            <h4 className="font-bold text-slate-900 text-sm mb-1">Local-First</h4>
            <p className="text-xs text-slate-600">Your data lives on your device only.</p>
          </div>
          <div className="text-center">
            <div className="w-12 h-12 rounded-full bg-purple-100 flex items-center justify-center mx-auto mb-3">
              <i className="fa-solid fa-eye-slash text-purple-600 text-xl"></i>
            </div>
            <h4 className="font-bold text-slate-900 text-sm mb-1">Zero Telemetry</h4>
            <p className="text-xs text-slate-600">No analytics, tracking, or data collection.</p>
          </div>
          <div className="text-center">
            <div className="w-12 h-12 rounded-full bg-orange-100 flex items-center justify-center mx-auto mb-3">
              <i className="fa-solid fa-sliders text-orange-600 text-xl"></i>
            </div>
            <h4 className="font-bold text-slate-900 text-sm mb-1">User Control</h4>
            <p className="text-xs text-slate-600">You decide what's captured and when.</p>
          </div>
        </div>
      </div>
    </div>
  </div>
);

const FeatureCard = ({ icon: Icon, title, description }: { icon: any, title: string, description: string }) => (
  <div className="p-8 rounded-2xl tech-panel hover:border-blue-300 hover:shadow-blue-100 transition-all group bg-white cursor-default">
    <div className="w-12 h-12 rounded-xl bg-blue-50 group-hover:bg-blue-100 flex items-center justify-center mb-6 transition-colors">
      <Icon className="w-6 h-6 text-blue-600 group-hover:scale-110 transition-transform duration-300" />
    </div>
    <h3 className="text-xl font-bold text-slate-900 mb-3">{title}</h3>
    <p className="text-slate-600 leading-relaxed text-sm font-medium">{description}</p>
  </div>
);

const Features = () => (
  <div id="features" className="py-24 bg-white border-t border-slate-100">
    <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
      <div className="text-center mb-16">
        <h2 className="text-3xl font-bold text-slate-900 sm:text-4xl">Powerful Screen Search Features</h2>
        <p className="mt-4 text-slate-600 text-lg">OCR-powered local screen indexing. Privacy-first, performance-optimized.</p>
      </div>
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-8">
        <FeatureCard
          icon={Clock}
          title="Screen History Timeline"
          description="Travel back through your screen history to find exactly what you were working on. Search by time, date, or application."
        />
        <FeatureCard
          icon={Search}
          title="OCR Screen Search"
          description="Full-text search across everything on your screen. Windows OCR extracts text from images, PDFs, and videos—all indexed locally with instant BM25-ranked results."
        />
        <FeatureCard
          icon={Lock}
          title="Local Screen Indexing"
          description="100% local-first architecture. Your screen history stays on your device forever. No cloud uploads, no telemetry, no tracking. You control your data."
        />
        <FeatureCard
          icon={Cpu}
          title="Local Intelligence"
          description="Optional local LLM integration to query your history without data leaving your machine."
        />
        <FeatureCard
          icon={Zap}
          title="Rust Native"
          description="Built with Rust for blazing fast performance, type safety, and minimal memory footprint."
        />
        <FeatureCard
          icon={GitBranch}
          title="Open Source"
          description="MIT Licensed. Audit the code, build from source, or contribute to the roadmap."
        />
      </div>
    </div>
  </div>
);

const FAQItem = ({ question, answer }: { question: string, answer: string | React.ReactNode }) => {
  const [isOpen, setIsOpen] = useState(false);

  return (
    <div className="border border-slate-200 rounded-xl overflow-hidden bg-white">
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="w-full px-6 py-5 text-left flex items-center justify-between hover:bg-slate-50 transition-colors"
      >
        <h3 className="font-bold text-slate-900 pr-8">{question}</h3>
        <i className={`fa-solid fa-chevron-down text-blue-600 transition-transform ${isOpen ? 'rotate-180' : ''}`}></i>
      </button>
      {isOpen && (
        <div className="px-6 pb-5 text-slate-700 leading-relaxed border-t border-slate-100 bg-slate-50/50">
          <div className="pt-4">{answer}</div>
        </div>
      )}
    </div>
  );
};

const FAQ = () => (
  <div id="faq" className="py-24 bg-white">
    <div className="max-w-4xl mx-auto px-4 sm:px-6 lg:px-8">
      <div className="text-center mb-12">
        <h2 className="text-3xl font-bold text-slate-900 sm:text-4xl mb-4">Frequently Asked Questions</h2>
        <p className="text-lg text-slate-600">Everything you need to know about ScreenSearch</p>
      </div>

      <div className="space-y-4">
        <FAQItem
          question="What is ScreenSearch?"
          answer="ScreenSearch is a free, open-source screen recording and indexing tool for Windows 10/11. It continuously captures your screen, extracts text using OCR (Optical Character Recognition), and lets you search everything you've ever seen on your computer—all stored locally on your device with zero cloud uploads."
        />

        <FAQItem
          question="How is ScreenSearch different from Microsoft Windows Recall?"
          answer={
            <div className="space-y-3">
              <p>While Windows Recall faced criticism for privacy concerns and security vulnerabilities, ScreenSearch is built privacy-first from the ground up:</p>
              <ul className="list-disc list-inside space-y-2 ml-2">
                <li><strong>100% Open Source:</strong> MIT licensed, fully auditable code on GitHub</li>
                <li><strong>Local-Only Storage:</strong> Your data never leaves your device</li>
                <li><strong>No Telemetry:</strong> We literally cannot see your data</li>
                <li><strong>Full User Control:</strong> Choose what to capture, exclude apps, delete anytime</li>
                <li><strong>Platform Support:</strong> Works on regular Windows 10/11, not just Copilot+ PCs</li>
                <li><strong>Free Forever:</strong> No subscriptions, no premium tiers</li>
              </ul>
            </div>
          }
        />

        <FAQItem
          question="Is ScreenSearch safe? Will it record passwords or sensitive information?"
          answer={
            <div className="space-y-3">
              <p>ScreenSearch gives you complete control over what gets captured:</p>
              <ul className="list-disc list-inside space-y-2 ml-2">
                <li><strong>App Exclusions:</strong> Prevent specific applications from being recorded (password managers, banking apps, private browsers)</li>
                <li><strong>Auto-Pause:</strong> Automatically pauses when screen is locked</li>
                <li><strong>Manual Pause:</strong> One-click pause anytime</li>
                <li><strong>Selective Deletion:</strong> Delete specific captures or time ranges</li>
                <li><strong>No Cloud Uploads:</strong> Everything stays on your device, encrypted at rest</li>
              </ul>
              <p className="mt-2 text-sm">We recommend excluding password managers and private browsing sessions in Settings.</p>
            </div>
          }
        />

        <FAQItem
          question="Does ScreenSearch slow down my computer?"
          answer={
            <div>
              <p className="mb-2">No. ScreenSearch is built in Rust for maximum performance:</p>
              <ul className="list-disc list-inside space-y-1 ml-2">
                <li><strong>&lt; 5% CPU usage</strong> when idle</li>
                <li><strong>&lt; 500MB RAM</strong> typical usage</li>
                <li><strong>&lt; 100ms API response times</strong></li>
                <li>Intelligent frame differencing (only captures when screen changes)</li>
                <li>Configurable capture intervals (2-5 seconds)</li>
              </ul>
              <p className="mt-2">Most users don't even notice it's running.</p>
            </div>
          }
        />

        <FAQItem
          question="What's the difference between ScreenSearch and Rewind?"
          answer={
            <div className="overflow-x-auto">
              <table className="w-full text-sm mt-2">
                <thead className="bg-slate-100">
                  <tr>
                    <th className="px-4 py-2 text-left font-bold">Feature</th>
                    <th className="px-4 py-2 text-left font-bold text-blue-600">ScreenSearch</th>
                    <th className="px-4 py-2 text-left font-bold">Rewind</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-100">
                  <tr><td className="px-4 py-2">Platform</td><td className="px-4 py-2 font-semibold">Windows 10/11</td><td className="px-4 py-2">macOS only</td></tr>
                  <tr><td className="px-4 py-2">Price</td><td className="px-4 py-2 font-semibold text-green-600">Free forever</td><td className="px-4 py-2">$20/month</td></tr>
                  <tr><td className="px-4 py-2">Open Source</td><td className="px-4 py-2 font-semibold">✅ MIT License</td><td className="px-4 py-2">❌ Proprietary</td></tr>
                  <tr><td className="px-4 py-2">Local Storage</td><td className="px-4 py-2 font-semibold">✅ 100%</td><td className="px-4 py-2">Cloud backup optional</td></tr>
                  <tr><td className="px-4 py-2">API Access</td><td className="px-4 py-2 font-semibold">✅ 27 endpoints</td><td className="px-4 py-2">Limited</td></tr>
                </tbody>
              </table>
            </div>
          }
        />

        <FAQItem
          question="Can I search for text from images, PDFs, and videos?"
          answer={
            <div>
              <p className="mb-2">Yes! ScreenSearch uses Windows OCR API to extract text from:</p>
              <ul className="list-disc list-inside space-y-1 ml-2">
                <li>Screenshots and images</li>
                <li>PDF documents displayed on screen</li>
                <li>Video captions and text overlays</li>
                <li>Web pages (including dynamic content)</li>
                <li>Application UIs and dialog boxes</li>
              </ul>
              <p className="mt-2">All text is indexed locally using SQLite FTS5 for instant search results.</p>
            </div>
          }
        />

        <FAQItem
          question="Is ScreenSearch really free? What's the catch?"
          answer={
            <div>
              <p className="mb-2">Yes, ScreenSearch is 100% free with no catch:</p>
              <ul className="list-disc list-inside space-y-1 ml-2">
                <li><strong>MIT Licensed:</strong> Free and open source forever</li>
                <li><strong>No Premium Tiers:</strong> All features available to everyone</li>
                <li><strong>No Data Collection:</strong> We don't monetize your data</li>
                <li><strong>No Ads:</strong> Clean, focused interface</li>
                <li><strong>Community-Driven:</strong> Built by developers, for developers</li>
              </ul>
              <p className="mt-2 font-semibold text-blue-600">We believe privacy tools should be accessible to everyone.</p>
            </div>
          }
        />

        <FAQItem
          question="How much storage does ScreenSearch use?"
          answer={
            <div>
              <p className="mb-2">Storage usage depends on your configuration and screen activity:</p>
              <ul className="list-disc list-inside space-y-1 ml-2">
                <li><strong>Typical usage:</strong> 2-5 GB per week</li>
                <li><strong>Configurable retention:</strong> Set automatic cleanup (7 days, 30 days, 90 days, unlimited)</li>
                <li><strong>Efficient compression:</strong> Frame differencing reduces redundancy</li>
                <li><strong>Selective deletion:</strong> Remove old or unnecessary captures anytime</li>
              </ul>
              <p className="mt-2">You have full control over storage limits in Settings.</p>
            </div>
          }
        />
      </div>
    </div>
  </div>
);

const ComparisonTable = () => (
  <div className="py-24 bg-slate-50/50">
    <div className="max-w-6xl mx-auto px-4 sm:px-6 lg:px-8">
      <div className="text-center mb-12">
        <h2 className="text-3xl font-bold text-slate-900 sm:text-4xl mb-4">How ScreenSearch Compares</h2>
        <p className="text-lg text-slate-600">See why developers choose ScreenSearch</p>
      </div>

      <div className="overflow-x-auto">
        <table className="w-full bg-white rounded-xl shadow-sm border border-slate-200">
          <thead>
            <tr className="border-b border-slate-200 bg-slate-50">
              <th className="px-6 py-4 text-left text-sm font-bold text-slate-900">Feature</th>
              <th className="px-6 py-4 text-left text-sm font-bold text-blue-600">ScreenSearch</th>
              <th className="px-6 py-4 text-left text-sm font-bold text-slate-700">Windows Recall</th>
              <th className="px-6 py-4 text-left text-sm font-bold text-slate-700">Rewind</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-100">
            <tr className="hover:bg-slate-50/50">
              <td className="px-6 py-4 text-sm font-medium text-slate-900">Platform</td>
              <td className="px-6 py-4 text-sm font-semibold text-blue-600">Windows 10/11</td>
              <td className="px-6 py-4 text-sm">Windows 11 Copilot+ only</td>
              <td className="px-6 py-4 text-sm">macOS</td>
            </tr>
            <tr className="hover:bg-slate-50/50">
              <td className="px-6 py-4 text-sm font-medium text-slate-900">Price</td>
              <td className="px-6 py-4 text-sm font-semibold text-green-600">Free</td>
              <td className="px-6 py-4 text-sm">Included</td>
              <td className="px-6 py-4 text-sm">$20/month</td>
            </tr>
            <tr className="hover:bg-slate-50/50 bg-blue-50/30">
              <td className="px-6 py-4 text-sm font-medium text-slate-900">Open Source</td>
              <td className="px-6 py-4 text-sm font-semibold text-blue-600">✅ MIT License</td>
              <td className="px-6 py-4 text-sm">❌ Proprietary</td>
              <td className="px-6 py-4 text-sm">❌ Proprietary</td>
            </tr>
            <tr className="hover:bg-slate-50/50 bg-blue-50/30">
              <td className="px-6 py-4 text-sm font-medium text-slate-900">Privacy</td>
              <td className="px-6 py-4 text-sm font-semibold text-blue-600">100% Local</td>
              <td className="px-6 py-4 text-sm">Local with concerns ⚠️</td>
              <td className="px-6 py-4 text-sm">Cloud backup ⚠️</td>
            </tr>
            <tr className="hover:bg-slate-50/50">
              <td className="px-6 py-4 text-sm font-medium text-slate-900">OCR Search</td>
              <td className="px-6 py-4 text-sm">✅</td>
              <td className="px-6 py-4 text-sm">✅</td>
              <td className="px-6 py-4 text-sm">✅</td>
            </tr>
            <tr className="hover:bg-slate-50/50 bg-blue-50/30">
              <td className="px-6 py-4 text-sm font-medium text-slate-900">REST API</td>
              <td className="px-6 py-4 text-sm font-semibold text-blue-600">✅ 27 endpoints</td>
              <td className="px-6 py-4 text-sm">❌</td>
              <td className="px-6 py-4 text-sm">Limited</td>
            </tr>
            <tr className="hover:bg-slate-50/50">
              <td className="px-6 py-4 text-sm font-medium text-slate-900">Multi-Monitor</td>
              <td className="px-6 py-4 text-sm">✅ Automatic</td>
              <td className="px-6 py-4 text-sm">✅</td>
              <td className="px-6 py-4 text-sm">✅</td>
            </tr>
            <tr className="hover:bg-slate-50/50 bg-blue-50/30">
              <td className="px-6 py-4 text-sm font-medium text-slate-900">App Exclusions</td>
              <td className="px-6 py-4 text-sm font-semibold text-blue-600">✅ Full control</td>
              <td className="px-6 py-4 text-sm">Limited</td>
              <td className="px-6 py-4 text-sm">Basic</td>
            </tr>
            <tr className="hover:bg-slate-50/50">
              <td className="px-6 py-4 text-sm font-medium text-slate-900">Hardware Requirements</td>
              <td className="px-6 py-4 text-sm font-semibold text-blue-600">Standard PC</td>
              <td className="px-6 py-4 text-sm text-red-600">NPU required</td>
              <td className="px-6 py-4 text-sm">Mac M1+</td>
            </tr>
          </tbody>
        </table>
      </div>

      <div className="mt-8 bg-white rounded-xl p-6 border border-blue-200 shadow-sm">
        <h3 className="font-bold text-lg text-slate-900 mb-3">Why Choose ScreenSearch?</h3>
        <div className="grid md:grid-cols-2 gap-4">
          <div className="flex items-start gap-2">
            <i className="fa-solid fa-check-circle text-green-600 mt-0.5"></i>
            <span className="text-sm text-slate-700"><strong>Privacy You Can Verify:</strong> Open source means security researchers can audit our code</span>
          </div>
          <div className="flex items-start gap-2">
            <i className="fa-solid fa-check-circle text-green-600 mt-0.5"></i>
            <span className="text-sm text-slate-700"><strong>Works on Any Windows PC:</strong> No expensive Copilot+ hardware required</span>
          </div>
          <div className="flex items-start gap-2">
            <i className="fa-solid fa-check-circle text-green-600 mt-0.5"></i>
            <span className="text-sm text-slate-700"><strong>No Subscriptions:</strong> One-time download, yours forever</span>
          </div>
          <div className="flex items-start gap-2">
            <i className="fa-solid fa-check-circle text-green-600 mt-0.5"></i>
            <span className="text-sm text-slate-700"><strong>Developer-Friendly:</strong> Full REST API for automation and integrations</span>
          </div>
        </div>
      </div>
    </div>
  </div>
);

const UseCases = () => (
  <div className="py-24 bg-white border-t border-slate-100">
    <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
      <div className="text-center mb-16">
        <h2 className="text-3xl font-bold text-slate-900 sm:text-4xl mb-4">Who Uses ScreenSearch?</h2>
        <p className="text-lg text-slate-600 max-w-3xl mx-auto">
          From developers to designers, ScreenSearch helps professionals find exactly what they need.
        </p>
      </div>

      <div className="grid md:grid-cols-2 gap-8">
        {/* Developers */}
        <div className="p-8 rounded-2xl bg-gradient-to-br from-blue-50 to-cyan-50 border border-blue-100">
          <div className="flex items-start gap-4 mb-4">
            <div className="w-12 h-12 rounded-xl bg-blue-600 flex items-center justify-center flex-shrink-0">
              <i className="fa-solid fa-code text-white text-xl"></i>
            </div>
            <div>
              <h3 className="text-xl font-bold text-slate-900 mb-2">Software Developers</h3>
              <p className="text-sm text-blue-900 italic">"What was that API endpoint I saw in the docs yesterday?"</p>
            </div>
          </div>
          <p className="text-slate-700 mb-4">Search your coding history—documentation, StackOverflow answers, terminal commands, code reviews.</p>
          <ul className="space-y-2 text-sm text-slate-600">
            <li className="flex items-start gap-2">
              <i className="fa-solid fa-check text-blue-600 mt-0.5"></i>
              <span>Find code snippets you saw but didn't bookmark</span>
            </li>
            <li className="flex items-start gap-2">
              <i className="fa-solid fa-check text-blue-600 mt-0.5"></i>
              <span>Search error messages and solutions</span>
            </li>
            <li className="flex items-start gap-2">
              <i className="fa-solid fa-check text-blue-600 mt-0.5"></i>
              <span>Track what you worked on for standup meetings</span>
            </li>
          </ul>
        </div>

        {/* Researchers */}
        <div className="p-8 rounded-2xl bg-gradient-to-br from-purple-50 to-pink-50 border border-purple-100">
          <div className="flex items-start gap-4 mb-4">
            <div className="w-12 h-12 rounded-xl bg-purple-600 flex items-center justify-center flex-shrink-0">
              <i className="fa-solid fa-microscope text-white text-xl"></i>
            </div>
            <div>
              <h3 className="text-xl font-bold text-slate-900 mb-2">Researchers & Academics</h3>
              <p className="text-sm text-purple-900 italic">"Where did I see that citation?"</p>
            </div>
          </div>
          <p className="text-slate-700 mb-4">Build a searchable archive of every research paper, article, and source you've viewed.</p>
          <ul className="space-y-2 text-sm text-slate-600">
            <li className="flex items-start gap-2">
              <i className="fa-solid fa-check text-purple-600 mt-0.5"></i>
              <span>Track citations across dozens of papers</span>
            </li>
            <li className="flex items-start gap-2">
              <i className="fa-solid fa-check text-purple-600 mt-0.5"></i>
              <span>Find figures and graphs you forgot to save</span>
            </li>
            <li className="flex items-start gap-2">
              <i className="fa-solid fa-check text-purple-600 mt-0.5"></i>
              <span>Search lecture slides and presentations</span>
            </li>
          </ul>
        </div>

        {/* Support Engineers */}
        <div className="p-8 rounded-2xl bg-gradient-to-br from-green-50 to-emerald-50 border border-green-100">
          <div className="flex items-start gap-4 mb-4">
            <div className="w-12 h-12 rounded-xl bg-green-600 flex items-center justify-center flex-shrink-0">
              <i className="fa-solid fa-headset text-white text-xl"></i>
            </div>
            <div>
              <h3 className="text-xl font-bold text-slate-900 mb-2">Support Engineers</h3>
              <p className="text-sm text-green-900 italic">"What error message did the customer send?"</p>
            </div>
          </div>
          <p className="text-slate-700 mb-4">Search support tickets, error logs, customer conversations, and shared screens instantly.</p>
          <ul className="space-y-2 text-sm text-slate-600">
            <li className="flex items-start gap-2">
              <i className="fa-solid fa-check text-green-600 mt-0.5"></i>
              <span>Find past support conversations by keyword</span>
            </li>
            <li className="flex items-start gap-2">
              <i className="fa-solid fa-check text-green-600 mt-0.5"></i>
              <span>Search error messages across reports</span>
            </li>
            <li className="flex items-start gap-2">
              <i className="fa-solid fa-check text-green-600 mt-0.5"></i>
              <span>Track recurring issues and patterns</span>
            </li>
          </ul>
        </div>

        {/* Designers */}
        <div className="p-8 rounded-2xl bg-gradient-to-br from-orange-50 to-amber-50 border border-orange-100">
          <div className="flex items-start gap-4 mb-4">
            <div className="w-12 h-12 rounded-xl bg-orange-600 flex items-center justify-center flex-shrink-0">
              <i className="fa-solid fa-palette text-white text-xl"></i>
            </div>
            <div>
              <h3 className="text-xl font-bold text-slate-900 mb-2">Designers & Creatives</h3>
              <p className="text-sm text-orange-900 italic">"I saw the perfect color palette but can't find it."</p>
            </div>
          </div>
          <p className="text-slate-700 mb-4">Create a visual reference library from everything you've browsed.</p>
          <ul className="space-y-2 text-sm text-slate-600">
            <li className="flex items-start gap-2">
              <i className="fa-solid fa-check text-orange-600 mt-0.5"></i>
              <span>Save design inspiration automatically</span>
            </li>
            <li className="flex items-start gap-2">
              <i className="fa-solid fa-check text-orange-600 mt-0.5"></i>
              <span>Search mockups and prototypes you've reviewed</span>
            </li>
            <li className="flex items-start gap-2">
              <i className="fa-solid fa-check text-orange-600 mt-0.5"></i>
              <span>Find UI patterns and layouts</span>
            </li>
          </ul>
        </div>

        {/* Analysts */}
        <div className="p-8 rounded-2xl bg-gradient-to-br from-indigo-50 to-blue-50 border border-indigo-100">
          <div className="flex items-start gap-4 mb-4">
            <div className="w-12 h-12 rounded-xl bg-indigo-600 flex items-center justify-center flex-shrink-0">
              <i className="fa-solid fa-chart-line text-white text-xl"></i>
            </div>
            <div>
              <h3 className="text-xl font-bold text-slate-900 mb-2">Analysts & Data Scientists</h3>
              <p className="text-sm text-indigo-900 italic">"Which dashboard showed that metric spike?"</p>
            </div>
          </div>
          <p className="text-slate-700 mb-4">Search charts, graphs, and data visualizations across all your tools.</p>
          <ul className="space-y-2 text-sm text-slate-600">
            <li className="flex items-start gap-2">
              <i className="fa-solid fa-check text-indigo-600 mt-0.5"></i>
              <span>Find specific charts and visualizations</span>
            </li>
            <li className="flex items-start gap-2">
              <i className="fa-solid fa-check text-indigo-600 mt-0.5"></i>
              <span>Track down data sources</span>
            </li>
            <li className="flex items-start gap-2">
              <i className="fa-solid fa-check text-indigo-600 mt-0.5"></i>
              <span>Search SQL queries and results</span>
            </li>
          </ul>
        </div>

        {/* Writers */}
        <div className="p-8 rounded-2xl bg-gradient-to-br from-rose-50 to-red-50 border border-rose-100">
          <div className="flex items-start gap-4 mb-4">
            <div className="w-12 h-12 rounded-xl bg-rose-600 flex items-center justify-center flex-shrink-0">
              <i className="fa-solid fa-pen-fancy text-white text-xl"></i>
            </div>
            <div>
              <h3 className="text-xl font-bold text-slate-900 mb-2">Writers & Content Creators</h3>
              <p className="text-sm text-rose-900 italic">"I read a great quote but didn't save the source."</p>
            </div>
          </div>
          <p className="text-slate-700 mb-4">Search every article, tweet, and document you've read.</p>
          <ul className="space-y-2 text-sm text-slate-600">
            <li className="flex items-start gap-2">
              <i className="fa-solid fa-check text-rose-600 mt-0.5"></i>
              <span>Find quotes and sources for content</span>
            </li>
            <li className="flex items-start gap-2">
              <i className="fa-solid fa-check text-rose-600 mt-0.5"></i>
              <span>Search research materials</span>
            </li>
            <li className="flex items-start gap-2">
              <i className="fa-solid fa-check text-rose-600 mt-0.5"></i>
              <span>Review draft revisions</span>
            </li>
          </ul>
        </div>
      </div>
    </div>
  </div>
);

// --- AI Demo Section ---

// --- Visual Showcase Section ---

const VisualShowcase = () => (
  <div id="demo" className="py-24 relative overflow-hidden bg-gradient-to-b from-slate-50 to-white">
    <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
      <div className="text-center mb-16">
        <div className="inline-flex items-center gap-2 px-4 py-2 rounded-full bg-blue-50 border border-blue-200 mb-6">
          <i className="fa-solid fa-wand-magic-sparkles text-blue-600"></i>
          <span className="text-sm font-semibold text-blue-900">See It In Action</span>
        </div>
        <h2 className="text-3xl font-bold text-slate-900 sm:text-4xl mb-4">
          Search Everything on Your Screen
        </h2>
        <p className="text-lg text-slate-600 max-w-3xl mx-auto">
          ScreenSearch captures your screen, extracts text with OCR, and indexes everything locally for instant search.
        </p>
      </div>

      <div className="grid md:grid-cols-3 gap-6 mb-12">
        {/* Example 1: Code Search */}
        <div className="bg-white rounded-2xl p-6 border border-slate-200 shadow-sm hover:shadow-lg transition-shadow">
          <div className="aspect-video bg-gradient-to-br from-blue-50 to-cyan-50 rounded-lg mb-4 flex items-center justify-center border border-blue-100">
            <i className="fa-solid fa-code text-6xl text-blue-400"></i>
          </div>
          <h3 className="font-bold text-slate-900 mb-2">Find Code Snippets</h3>
          <p className="text-sm text-slate-600 mb-3">Search "API endpoint" to find that documentation you saw yesterday</p>
          <div className="bg-slate-50 rounded-lg p-3 border border-slate-200">
            <code className="text-xs text-slate-700 font-mono">
              → Search: "fetch users API"<br />
              ✓ Found in 3 screens<br />
              📄 docs.example.com (2 hours ago)
            </code>
          </div>
        </div>

        {/* Example 2: Error Messages */}
        <div className="bg-white rounded-2xl p-6 border border-slate-200 shadow-sm hover:shadow-lg transition-shadow">
          <div className="aspect-video bg-gradient-to-br from-red-50 to-orange-50 rounded-lg mb-4 flex items-center justify-center border border-red-100">
            <i className="fa-solid fa-triangle-exclamation text-6xl text-red-400"></i>
          </div>
          <h3 className="font-bold text-slate-900 mb-2">Track Error Messages</h3>
          <p className="text-sm text-slate-600 mb-3">Search error codes to find solutions you've already seen</p>
          <div className="bg-slate-50 rounded-lg p-3 border border-slate-200">
            <code className="text-xs text-slate-700 font-mono">
              → Search: "TypeError undefined"<br />
              ✓ Found in 7 screens<br />
              📄 stackoverflow.com (yesterday)
            </code>
          </div>
        </div>

        {/* Example 3: Design Inspiration */}
        <div className="bg-white rounded-2xl p-6 border border-slate-200 shadow-sm hover:shadow-lg transition-shadow">
          <div className="aspect-video bg-gradient-to-br from-purple-50 to-pink-50 rounded-lg mb-4 flex items-center justify-center border border-purple-100">
            <i className="fa-solid fa-palette text-6xl text-purple-400"></i>
          </div>
          <h3 className="font-bold text-slate-900 mb-2">Save Design Ideas</h3>
          <p className="text-sm text-slate-600 mb-3">Search color codes or design patterns you browsed</p>
          <div className="bg-slate-50 rounded-lg p-3 border border-slate-200">
            <code className="text-xs text-slate-700 font-mono">
              → Search: "#4F46E5 gradient"<br />
              ✓ Found in 5 screens<br />
              📄 dribbble.com (last week)
            </code>
          </div>
        </div>
      </div>

      {/* How It Works */}
      <div className="bg-white rounded-2xl p-8 border border-slate-200 shadow-sm">
        <h3 className="text-xl font-bold text-slate-900 mb-6 text-center">How It Works</h3>
        <div className="grid md:grid-cols-4 gap-6">
          <div className="text-center">
            <div className="w-16 h-16 rounded-full bg-blue-100 flex items-center justify-center mx-auto mb-4">
              <span className="text-2xl font-bold text-blue-600">1</span>
            </div>
            <h4 className="font-bold text-slate-900 mb-2">Capture</h4>
            <p className="text-sm text-slate-600">Continuously screenshots your screen every few seconds</p>
          </div>
          <div className="text-center">
            <div className="w-16 h-16 rounded-full bg-purple-100 flex items-center justify-center mx-auto mb-4">
              <span className="text-2xl font-bold text-purple-600">2</span>
            </div>
            <h4 className="font-bold text-slate-900 mb-2">Extract</h4>
            <p className="text-sm text-slate-600">Windows OCR extracts all text from screenshots</p>
          </div>
          <div className="text-center">
            <div className="w-16 h-16 rounded-full bg-green-100 flex items-center justify-center mx-auto mb-4">
              <span className="text-2xl font-bold text-green-600">3</span>
            </div>
            <h4 className="font-bold text-slate-900 mb-2">Index</h4>
            <p className="text-sm text-slate-600">Indexes text locally in SQLite with FTS5</p>
          </div>
          <div className="text-center">
            <div className="w-16 h-16 rounded-full bg-orange-100 flex items-center justify-center mx-auto mb-4">
              <span className="text-2xl font-bold text-orange-600">4</span>
            </div>
            <h4 className="font-bold text-slate-900 mb-2">Search</h4>
            <p className="text-sm text-slate-600">Instant full-text search with BM25 ranking</p>
          </div>
        </div>
      </div>

      {/* CTA */}
      <div className="mt-12 text-center">
        <div className="inline-flex flex-col items-center gap-4">
          <p className="text-slate-600">Ready to search your entire screen history?</p>
          <a
            href="https://github.com/nicolasestrem/screensearch/releases"
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-2 px-8 py-4 bg-gradient-to-r from-blue-600 to-cyan-500 text-white font-bold rounded-xl hover:shadow-xl hover:scale-105 transition-all duration-300"
          >
            <i className="fa-brands fa-github text-xl"></i>
            <span>Download for Windows</span>
            <i className="fa-solid fa-arrow-right"></i>
          </a>
          <p className="text-xs text-slate-500">Free & open source • Windows 10/11</p>
        </div>
      </div>
    </div>
  </div>
);

const DemoSection = () => {
  const [image, setImage] = useState<string | null>(null);
  const [prompt, setPrompt] = useState("");
  const [response, setResponse] = useState("");
  const [loading, setLoading] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const handleImageUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) {
      const reader = new FileReader();
      reader.onloadend = () => {
        setImage(reader.result as string);
        setResponse("");
      };
      reader.readAsDataURL(file);
    }
  };

  const analyzeImage = async () => {
    if (!image || !process.env.API_KEY) {
      if (!process.env.API_KEY) alert("API Key not configured in environment.");
      return;
    }

    setLoading(true);
    setResponse("");

    try {
      const ai = new GoogleGenAI({ apiKey: process.env.API_KEY });
      // Clean base64 string
      const base64Data = image.split(',')[1];

      const contents = {
        parts: [
          {
            inlineData: {
              mimeType: "image/jpeg", // Assuming jpeg/png, API handles standard types
              data: base64Data
            }
          },
          {
            text: prompt || "Describe what is happening on this screen in detail. Identify any code, text, or apps visible."
          }
        ]
      };

      const result = await ai.models.generateContent({
        model: 'gemini-2.5-flash',
        contents: contents
      });

      setResponse(result.text || "No response generated.");
    } catch (error) {
      console.error(error);
      setResponse("Error analyzing image. Please try again.");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div id="demo" className="py-24 relative overflow-hidden bg-slate-50/50">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
        <div className="bg-white rounded-2xl md:rounded-3xl p-6 md:p-12 border border-slate-200 shadow-2xl shadow-blue-100/50">
          <div className="grid md:grid-cols-2 gap-8 md:gap-12 items-center">
            {/* Left Column: Controls */}
            <div>
              <div className="inline-flex items-center gap-2 mb-4 px-3 py-1 bg-blue-50 text-blue-700 rounded-full text-xs font-bold uppercase tracking-wider">
                <i className="fa-solid fa-bolt"></i> Live Demo
              </div>
              <h2 className="text-3xl font-extrabold text-slate-900 mb-6">
                Test the Intelligence
              </h2>
              <p className="text-slate-600 mb-8 leading-relaxed font-medium">
                Experience the recognition engine firsthand. Upload a screenshot to see how Screen Memory parses context, code, and UI elements in real-time.
              </p>

              <div className="space-y-4">
                <div
                  onClick={() => fileInputRef.current?.click()}
                  className={`border-2 border-dashed rounded-xl p-8 text-center cursor-pointer transition-all duration-300 group ${image ? 'border-blue-500 bg-blue-50' : 'border-slate-300 hover:border-blue-400 hover:bg-slate-50'}`}
                >
                  <input
                    type="file"
                    ref={fileInputRef}
                    className="hidden"
                    accept="image/*"
                    onChange={handleImageUpload}
                  />
                  {image ? (
                    <div className="relative">
                      <img src={image} alt="Preview" className="max-h-48 mx-auto rounded-lg shadow-md border border-slate-200" />
                      <div className="mt-3 text-blue-700 text-sm font-bold flex items-center justify-center gap-2">
                        <i className="fa-solid fa-circle-check"></i> Image Loaded
                      </div>
                    </div>
                  ) : (
                    <div className="text-slate-500 group-hover:text-blue-500 transition-colors">
                      <div className="w-16 h-16 bg-slate-100 group-hover:bg-blue-100 rounded-full flex items-center justify-center mx-auto mb-4 text-slate-400 group-hover:text-blue-500 transition-colors">
                        <i className="fa-solid fa-cloud-arrow-up text-2xl"></i>
                      </div>
                      <p className="font-semibold text-slate-700 group-hover:text-blue-700">Click to upload screenshot</p>
                      <p className="text-xs text-slate-400 mt-1">PNG, JPG up to 5MB</p>
                    </div>
                  )}
                </div>

                <div className="flex flex-col sm:flex-row gap-2">
                  <input
                    type="text"
                    value={prompt}
                    onChange={(e) => setPrompt(e.target.value)}
                    placeholder="Ask a question..."
                    className="flex-1 bg-white border border-slate-300 rounded-xl px-4 py-4 text-slate-900 focus:outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-200 transition-all font-medium placeholder:text-slate-400"
                  />
                  <button
                    onClick={analyzeImage}
                    disabled={!image || loading}
                    className={`px-8 py-4 rounded-xl font-bold transition-all flex items-center justify-center gap-2 shadow-lg ${!image || loading ? 'bg-slate-100 text-slate-400 cursor-not-allowed shadow-none' : 'bg-blue-600 hover:bg-blue-500 text-white shadow-blue-500/30 hover:scale-105'}`}
                  >
                    {loading ? <i className="fa-solid fa-circle-notch fa-spin"></i> : <i className="fa-solid fa-wand-magic-sparkles"></i>}
                    Analyze
                  </button>
                </div>
              </div>
            </div>

            {/* Right Column: Terminal - Responsive Height */}
            <div className="relative w-full h-96 md:h-[600px] mt-8 md:mt-0">
              {/* Terminal Window - Dark Blue/Slate for High Contrast */}
              <div className="absolute inset-0 bg-[#0f172a] rounded-2xl border border-slate-700 overflow-hidden flex flex-col font-mono shadow-2xl">
                <div className="bg-[#1e293b] px-5 py-3 border-b border-slate-700 flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <i className="fa-solid fa-terminal text-blue-400 text-xs"></i>
                    <span className="text-xs text-slate-300 font-bold tracking-wide">ANALYSIS_OUTPUT.JSON</span>
                  </div>
                  <div className="flex gap-2">
                    <div className="w-3 h-3 rounded-full bg-slate-600 hover:bg-red-500 transition-colors"></div>
                    <div className="w-3 h-3 rounded-full bg-slate-600 hover:bg-yellow-500 transition-colors"></div>
                    <div className="w-3 h-3 rounded-full bg-slate-600 hover:bg-green-500 transition-colors"></div>
                  </div>
                </div>
                <div className="p-6 overflow-y-auto flex-1 text-sm bg-[#0f172a] scrollbar-thin scrollbar-thumb-slate-700 scrollbar-track-transparent">
                  {response ? (
                    <div className="text-slate-200 whitespace-pre-wrap animate-fadeIn leading-relaxed">
                      <span className="text-emerald-400 font-bold">➜</span> <span className="text-blue-400 font-bold">~</span> {response}
                    </div>
                  ) : (
                    <div className="h-full flex flex-col items-center justify-center text-slate-500">
                      <div className="w-20 h-20 rounded-full bg-slate-800/50 flex items-center justify-center mb-6 animate-pulse">
                        <Activity className="w-10 h-10 text-slate-600" />
                      </div>
                      <p className="font-medium tracking-wide text-center px-4">WAITING FOR INPUT STREAM...</p>
                    </div>
                  )}
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

const Footer = () => (
  <footer className="border-t border-slate-200 bg-white py-16">
    <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
      <div className="grid grid-cols-1 md:grid-cols-4 gap-12">
        <div className="col-span-1 md:col-span-2">
          <div className="mb-6">
            <Logo />
          </div>
          <p className="text-slate-500 text-sm max-w-sm leading-relaxed font-medium">
            Open source local screen indexing. <br />
            Search your digital history without sacrificing privacy.
          </p>
          <div className="mt-6 flex gap-4">
            <a href="https://github.com/nicolasestrem/screensearch" target="_blank" rel="noopener noreferrer" className="w-8 h-8 rounded-full bg-slate-100 flex items-center justify-center text-slate-500 hover:bg-blue-100 hover:text-blue-600 transition-colors">
              <Github className="w-4 h-4" />
            </a>
          </div>
        </div>
        <div>
          <h4 className="text-slate-900 font-bold mb-6 text-sm uppercase tracking-wider">Project</h4>
          <ul className="space-y-4 text-sm text-slate-600 font-medium">
            <li><a href="https://github.com/nicolasestrem/screensearch/releases" target="_blank" rel="noopener noreferrer" className="hover:text-blue-600 transition-colors flex items-center gap-2"><i className="fa-brands fa-windows"></i> Download Windows</a></li>
            <li><a href="https://github.com/nicolasestrem/screensearch" target="_blank" rel="noopener noreferrer" className="hover:text-blue-600 transition-colors">GitHub Repository</a></li>
            <li><a href="https://github.com/nicolasestrem/screensearch#readme" target="_blank" rel="noopener noreferrer" className="hover:text-blue-600 transition-colors">Documentation</a></li>
            <li><a href="https://github.com/nicolasestrem/screensearch/blob/main/LICENSE" target="_blank" rel="noopener noreferrer" className="hover:text-blue-600 transition-colors">License (MIT)</a></li>
          </ul>
        </div>
      </div>
      <div className="mt-16 pt-8 border-t border-slate-100 text-center text-slate-400 text-sm font-medium">
        &copy; {new Date().getFullYear()} ScreenSearch Contributors.
      </div>
    </div>
  </footer>
);

const App = () => {
  return (
    <div className="min-h-screen bg-white text-slate-900 selection:bg-blue-100 selection:text-blue-900">
      <Header />
      <main>
        <Hero />
        <WhyScreenSearch />
        <Features />
        <UseCases />
        <ComparisonTable />
        <FAQ />
        <VisualShowcase />
      </main>
      <Footer />
    </div>
  );
};

const root = createRoot(document.getElementById("root")!);
root.render(<App />);
