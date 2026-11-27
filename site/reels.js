// The clips: each is a button that plays its video and pauses it again.
// Nothing plays until someone asks, and the button says which it will do.

if (typeof document !== 'undefined') {
  for (const button of document.querySelectorAll('button.reel')) {
    const video = button.querySelector('video');
    if (!video) continue;
    const name = (button.getAttribute('aria-label') ?? '').replace(/^(Play|Pause): /, '');
    const label = () => button.setAttribute('aria-label', `${video.paused ? 'Play' : 'Pause'}: ${name}`);
    button.addEventListener('click', () => {
      if (video.paused) {
        // One clip at a time.
        for (const other of document.querySelectorAll('button.reel video')) if (other !== video) other.pause();
        void video.play();
      } else {
        video.pause();
      }
    });
    video.addEventListener('play', () => { button.classList.add('playing'); label(); });
    video.addEventListener('pause', () => { button.classList.remove('playing'); label(); });
    video.addEventListener('ended', () => { video.currentTime = 0; });
  }
}
