// ScreenSearch Download Page Interactive Features

document.addEventListener('DOMContentLoaded', () => {
    // Smooth scrolling for anchor links
    document.querySelectorAll('a[href^="#"]').forEach(anchor => {
        anchor.addEventListener('click', function (e) {
            e.preventDefault();
            const target = document.querySelector(this.getAttribute('href'));
            if (target) {
                target.scrollIntoView({
                    behavior: 'smooth',
                    block: 'start'
                });
            }
        });
    });

    // Track download clicks
    document.querySelectorAll('.btn-download').forEach(button => {
        button.addEventListener('click', (e) => {
            const downloadType = e.target.closest('.download-card').querySelector('h3').textContent;
            console.log(`Download started: ${downloadType}`);

            // Show download confirmation
            showNotification(`Downloading ${downloadType}...`);
        });
    });

    // FAQ expand/collapse tracking
    document.querySelectorAll('.faq-item').forEach(item => {
        item.addEventListener('toggle', (e) => {
            if (e.target.open) {
                const question = e.target.querySelector('summary').textContent;
                console.log(`FAQ opened: ${question}`);
            }
        });
    });

    // Detect OS and highlight recommended download
    detectOS();

    // Add copy functionality to code blocks
    addCopyButtons();
});

// Detect user's operating system
function detectOS() {
    const userAgent = window.navigator.userAgent.toLowerCase();
    const platform = window.navigator.platform.toLowerCase();

    let os = 'unknown';

    if (platform.includes('win')) {
        os = 'windows';
    } else if (platform.includes('mac')) {
        os = 'macos';
    } else if (platform.includes('linux')) {
        os = 'linux';
    }

    // Show warning for non-Windows users
    if (os !== 'windows') {
        showOSWarning(os);
    }
}

// Show OS compatibility warning
function showOSWarning(os) {
    const hero = document.querySelector('.hero-content');

    const warning = document.createElement('div');
    warning.className = 'os-warning';
    warning.style.cssText = `
        background: rgba(245, 158, 11, 0.2);
        border: 2px solid rgba(245, 158, 11, 0.5);
        padding: 1rem;
        border-radius: 8px;
        margin-top: 2rem;
        text-align: center;
    `;

    let osName = os === 'macos' ? 'macOS' : os === 'linux' ? 'Linux' : 'your operating system';
    warning.innerHTML = `
        <strong>Note:</strong> ScreenSearch is currently Windows-only.
        ${osName} is not supported at this time.
    `;

    hero.appendChild(warning);
}

// Show notification message
function showNotification(message) {
    const notification = document.createElement('div');
    notification.className = 'notification';
    notification.textContent = message;
    notification.style.cssText = `
        position: fixed;
        top: 20px;
        right: 20px;
        background: #10b981;
        color: white;
        padding: 1rem 2rem;
        border-radius: 8px;
        box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
        z-index: 1000;
        animation: slideIn 0.3s ease-out;
    `;

    document.body.appendChild(notification);

    setTimeout(() => {
        notification.style.animation = 'slideOut 0.3s ease-out';
        setTimeout(() => notification.remove(), 300);
    }, 3000);
}

// Add copy buttons to code blocks
function addCopyButtons() {
    const codeBlocks = document.querySelectorAll('code');

    codeBlocks.forEach(codeBlock => {
        const container = codeBlock.parentElement;
        container.style.position = 'relative';

        const copyButton = document.createElement('button');
        copyButton.textContent = 'Copy';
        copyButton.className = 'copy-button';
        copyButton.style.cssText = `
            position: absolute;
            top: 10px;
            right: 10px;
            background: rgba(255, 255, 255, 0.2);
            color: white;
            border: 1px solid rgba(255, 255, 255, 0.3);
            padding: 0.25rem 0.75rem;
            border-radius: 4px;
            cursor: pointer;
            font-size: 0.875rem;
            transition: all 0.2s;
        `;

        copyButton.addEventListener('click', () => {
            const text = codeBlock.textContent;
            navigator.clipboard.writeText(text).then(() => {
                copyButton.textContent = 'Copied!';
                copyButton.style.background = 'rgba(16, 185, 129, 0.8)';

                setTimeout(() => {
                    copyButton.textContent = 'Copy';
                    copyButton.style.background = 'rgba(255, 255, 255, 0.2)';
                }, 2000);
            });
        });

        copyButton.addEventListener('mouseenter', () => {
            copyButton.style.background = 'rgba(255, 255, 255, 0.3)';
        });

        copyButton.addEventListener('mouseleave', () => {
            if (copyButton.textContent === 'Copy') {
                copyButton.style.background = 'rgba(255, 255, 255, 0.2)';
            }
        });

        container.appendChild(copyButton);
    });
}

// Add animations
const style = document.createElement('style');
style.textContent = `
    @keyframes slideIn {
        from {
            transform: translateX(400px);
            opacity: 0;
        }
        to {
            transform: translateX(0);
            opacity: 1;
        }
    }

    @keyframes slideOut {
        from {
            transform: translateX(0);
            opacity: 1;
        }
        to {
            transform: translateX(400px);
            opacity: 0;
        }
    }
`;
document.head.appendChild(style);
