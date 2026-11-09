-------------------------------------
Translated Report (Full Report Below)
-------------------------------------
Process:             REAPER [64971]
Path:                /Applications/REAPER.app/Contents/MacOS/REAPER
Identifier:          com.cockos.reaper
Version:             7.76.0_6cb0d69u (7.76.0_6cb0d69u)
Code Type:           ARM-64 (Native)
Role:                Foreground
Parent Process:      launchd [1]
Coalition:           com.cockos.reaper [2349]
User ID:             501

Date/Time:           2026-11-09 12:23:02.5650 +0700
Launch Time:         2026-11-09 12:16:28.9903 +0700
Hardware Model:      Mac16,1
OS Version:          macOS 27.0.1 (26A434)
Release Type:        User

Crash Reporter Key:  BCAEB6EC-392E-1692-8605-C8DE47F2E63E
Incident Identifier: 9A940F2A-0125-45AD-9DFD-3E4A04624005

Sleep/Wake UUID:       A67ADBBD-58F8-4755-A606-8BA2EECBD9F6

Time Awake Since Boot: 680000 seconds
Time Since Wake:       4922 seconds

System Integrity Protection: enabled

Triggered by Thread: 0  reaper, Dispatch Queue: com.apple.main-thread

Exception Type:    EXC_CRASH (SIGABRT)
Exception Codes:   0x0000000000000000, 0x0000000000000000

Termination Reason:  Namespace SIGNAL, Code 6, Abort trap: 6
Terminating Process: REAPER [64971]


Application Specific Information:
abort() called


Thread 0 Crashed:: reaper Dispatch queue: com.apple.main-thread
0   libsystem_kernel.dylib        	       0x1862d1650 __pthread_kill + 8
1   libsystem_pthread.dylib       	       0x18630d97c pthread_kill + 296
2   libsystem_c.dylib             	       0x186210458 abort + 148
3   satcomp                       	      0x705555b39c 0x70553a4000 + 1799068
4   satcomp                       	      0x705555b1ac 0x70553a4000 + 1798572
5   satcomp                       	      0x7055519d0c 0x70553a4000 + 1531148
6   satcomp                       	      0x7055505170 0x70553a4000 + 1446256
7   satcomp                       	      0x70554fd648 0x70553a4000 + 1414728
8   satcomp                       	      0x70555058d0 0x70553a4000 + 1448144
9   satcomp                       	      0x705555bbd8 0x70553a4000 + 1801176
10  satcomp                       	      0x705555bb24 0x70553a4000 + 1800996
11  satcomp                       	      0x705555bcb4 0x70553a4000 + 1801396
12  satcomp                       	      0x705542c668 0x70553a4000 + 558696
13  CoreFoundation                	       0x1863c31a0 __CFNOTIFICATIONCENTER_IS_CALLING_OUT_TO_AN_OBSERVER__ + 148
14  CoreFoundation                	       0x186452394 ___CFXRegistrationPost_block_invoke + 92
15  CoreFoundation                	       0x1864522d8 _CFXRegistrationPost + 440
16  CoreFoundation                	       0x18638f3d0 _CFXNotificationPost + 688
17  Foundation                    	       0x187a8038c -[NSNotificationCenter postNotificationName:object:userInfo:] + 88
18  AppKit                        	       0x18aad82cc -[NSWindow resignKeyWindow] + 636
19  REAPER                        	       0x103363bf4 -[REAPERSwell_modelesswindow resignKeyWindow] + 52
20  AppKit                        	       0x18b3c7640 -[NSWindow _sendResignKeyWindow] + 100
21  AppKit                        	       0x18aa7ca80 -[NSWindow _changeKeyAndMainLimitedOK:] + 612
22  AppKit                        	       0x18aa7c7f4 -[NSWindow makeKeyWindow] + 144
23  AppKit                        	       0x18aa7c5ac -[NSWindow _makeKeyRegardlessOfVisibility] + 56
24  AppKit                        	       0x18abaf3c8 -[NSApplication _orderFrontModalWindow:relativeToWindow:] + 508
25  AppKit                        	       0x18abaeff0 -[NSApplication _commonBeginModalSessionForWindow:relativeToWindow:modalDelegate:didEndSelector:contextInfo:] + 1368
26  AppKit                        	       0x18abaea10 __35-[NSApplication runModalForWindow:]_block_invoke_2 + 36
27  AppKit                        	       0x18abae9d0 __35-[NSApplication runModalForWindow:]_block_invoke + 108
28  AppKit                        	       0x18abae280 _NSTryRunModal + 100
29  AppKit                        	       0x18abae15c -[NSApplication runModalForWindow:] + 276
30  AppKit                        	       0x18b5aeeec -[NSSavePanel runModal] + 336
31  satcomp                       	      0x705541b0bc 0x70553a4000 + 487612
32  satcomp                       	      0x7055418a40 0x70553a4000 + 477760
33  satcomp                       	      0x70553e73d8 0x70553a4000 + 275416
34  satcomp                       	      0x70553b4750 0x70553a4000 + 67408
35  satcomp                       	      0x70553f6b40 0x70553a4000 + 338752
36  satcomp                       	      0x7055417b8c 0x70553a4000 + 473996
37  satcomp                       	      0x705542b0b0 0x70553a4000 + 553136
38  CoreFoundation                	       0x1863e88cc __CFRUNLOOP_IS_CALLING_OUT_TO_A_TIMER_CALLBACK_FUNCTION__ + 32
39  CoreFoundation                	       0x1863e85c8 __CFRunLoopDoTimer + 1084
40  CoreFoundation                	       0x1863e80d8 __CFRunLoopDoTimers + 280
41  CoreFoundation                	       0x1863cd5dc __CFRunLoopRun + 1852
42  CoreFoundation                	       0x1864a782c _CFRunLoopRunSpecificWithOptions + 536
43  HIToolbox                     	       0x193800590 RunCurrentEventLoopInMode + 320
44  HIToolbox                     	       0x19380391c ReceiveNextEventCommon + 492
45  HIToolbox                     	       0x193986380 _BlockUntilNextEventMatchingListInMode + 48
46  AppKit                        	       0x18acc729c _DPSBlockUntilNextEventMatchingListInMode + 292
47  AppKit                        	       0x18a9844fc _DPSNextEvent + 580
48  AppKit                        	       0x18b1fc748 -[NSApplication(NSEventRouting) _nextEventMatchingEventMask:untilDate:inMode:dequeue:] + 780
49  AppKit                        	       0x18b1fc26c -[NSApplication(NSEventRouting) nextEventMatchingMask:untilDate:inMode:dequeue:] + 292
50  AppKit                        	       0x18a97790c -[NSApplication run] + 396
51  AppKit                        	       0x18a94fc4c NSApplicationMain + 880
52  dyld                          	       0x185f37e80 start + 6688

Thread 1:: com.apple.NSEventThread
0   libsystem_kernel.dylib        	       0x1862c8c20 mach_msg2_trap + 8
1   libsystem_kernel.dylib        	       0x1862db8e0 mach_msg2_internal + 76
2   libsystem_kernel.dylib        	       0x1862d1a28 mach_msg_overwrite + 480
3   libsystem_kernel.dylib        	       0x1862c8fac mach_msg + 24
4   CoreFoundation                	       0x1863ceaec __CFRunLoopServiceMachPort + 160
5   CoreFoundation                	       0x1863cd334 __CFRunLoopRun + 1172
6   CoreFoundation                	       0x1864a782c _CFRunLoopRunSpecificWithOptions + 536
7   AppKit                        	       0x18aaa8844 _NSEventThread + 184
8   libsystem_pthread.dylib       	       0x18630dd00 _pthread_start + 136
9   libsystem_pthread.dylib       	       0x186308cec thread_start + 8

Thread 2:

Thread 3:
0   libsystem_kernel.dylib        	       0x1862c8c20 mach_msg2_trap + 8
1   libsystem_kernel.dylib        	       0x1862db8e0 mach_msg2_internal + 76
2   libsystem_kernel.dylib        	       0x1862d1a28 mach_msg_overwrite + 480
3   libsystem_kernel.dylib        	       0x1862c8fac mach_msg + 24
4   CoreMIDI                      	       0x1a7ce9128 XServerMachPort::ReceiveMessage(int&, void*, int&) + 104
5   CoreMIDI                      	       0x1a7d0adf4 MIDIProcess::MIDIInPortThread::Run() + 148
6   CoreMIDI                      	       0x1a7cfe4d4 CADeprecated::XThread::RunHelper(void*) + 40
7   CoreMIDI                      	       0x1a7d09e14 CADeprecated::CAPThread::Entry(void*) + 96
8   libsystem_pthread.dylib       	       0x18630dd00 _pthread_start + 136
9   libsystem_pthread.dylib       	       0x186308cec thread_start + 8

Thread 4:: */video
0   libsystem_kernel.dylib        	       0x1862cc308 __semwait_signal + 8
1   libsystem_c.dylib             	       0x1861a3054 nanosleep + 220
2   libsystem_c.dylib             	       0x1861a2f6c usleep + 68
3   reaper_video.dylib            	       0x12c017f2c videoDecodeThread(void*) + 1300
4   REAPER                        	       0x103390b38 __threadproc(void*) + 32
5   libsystem_pthread.dylib       	       0x18630dd00 _pthread_start + 136
6   libsystem_pthread.dylib       	       0x186308cec thread_start + 8

Thread 5:
0   libsystem_kernel.dylib        	       0x1862cc50c __psynch_cvwait + 8
1   libsystem_pthread.dylib       	       0x18630e1d0 _pthread_cond_wait + 980
2   libc++.1.dylib                	       0x18621e7bc std::__1::condition_variable::wait(std::__1::unique_lock<std::__1::mutex>&) + 32
3   Youlean Loudness Meter 2      	      0x701a6bfb28 0x701a680000 + 260904
4   Youlean Loudness Meter 2      	      0x701a6bf9d0 0x701a680000 + 260560
5   libsystem_pthread.dylib       	       0x18630dd00 _pthread_start + 136
6   libsystem_pthread.dylib       	       0x186308cec thread_start + 8

Thread 6:
0   libsystem_kernel.dylib        	       0x1862cc50c __psynch_cvwait + 8
1   libsystem_pthread.dylib       	       0x18630e1d0 _pthread_cond_wait + 980
2   libc++.1.dylib                	       0x18621e7bc std::__1::condition_variable::wait(std::__1::unique_lock<std::__1::mutex>&) + 32
3   Youlean Loudness Meter 2      	      0x701a6bfb28 0x701a680000 + 260904
4   Youlean Loudness Meter 2      	      0x701a6bf9d0 0x701a680000 + 260560
5   libsystem_pthread.dylib       	       0x18630dd00 _pthread_start + 136
6   libsystem_pthread.dylib       	       0x186308cec thread_start + 8

Thread 7:: JUCE v8.0.12: Timer
0   libsystem_kernel.dylib        	       0x1862cc50c __psynch_cvwait + 8
1   libsystem_pthread.dylib       	       0x18630e1d0 _pthread_cond_wait + 980
2   libc++.1.dylib                	       0x18621e854 std::__1::condition_variable::__do_timed_wait(std::__1::unique_lock<std::__1::mutex>&, std::__1::chrono::time_point<std::__1::chrono::system_clock, std::__1::chrono::duration<long long, std::__1::ratio<1l, 1000000000l>>>) + 104
3   Scyllascope                   	      0x701024a8a4 juce::WaitableEvent::wait(double) const + 576
4   Scyllascope                   	      0x701023fb4c juce::Timer::TimerThread::run() + 572
5   Scyllascope                   	      0x7010272c5c juce::Thread::createNativeThread(juce::Thread::Priority)::$_0::__invoke(void*) + 684
6   libsystem_pthread.dylib       	       0x18630dd00 _pthread_start + 136
7   libsystem_pthread.dylib       	       0x186308cec thread_start + 8

Thread 8:
0   libsystem_kernel.dylib        	       0x1862cc50c __psynch_cvwait + 8
1   libsystem_pthread.dylib       	       0x18630e1d0 _pthread_cond_wait + 980
2   libc++.1.dylib                	       0x18621e7bc std::__1::condition_variable::wait(std::__1::unique_lock<std::__1::mutex>&) + 32
3   Youlean Loudness Meter 2      	      0x701a6bfb28 0x701a680000 + 260904
4   Youlean Loudness Meter 2      	      0x701a6bf9d0 0x701a680000 + 260560
5   libsystem_pthread.dylib       	       0x18630dd00 _pthread_start + 136
6   libsystem_pthread.dylib       	       0x186308cec thread_start + 8

Thread 9:
0   libsystem_kernel.dylib        	       0x1862cc50c __psynch_cvwait + 8
1   libsystem_pthread.dylib       	       0x18630e1d0 _pthread_cond_wait + 980
2   libc++.1.dylib                	       0x18621e7bc std::__1::condition_variable::wait(std::__1::unique_lock<std::__1::mutex>&) + 32
3   Youlean Loudness Meter 2      	      0x701a6bfb28 0x701a680000 + 260904
4   Youlean Loudness Meter 2      	      0x701a6bf9d0 0x701a680000 + 260560
5   libsystem_pthread.dylib       	       0x18630dd00 _pthread_start + 136
6   libsystem_pthread.dylib       	       0x186308cec thread_start + 8

Thread 10:: bg-worker
0   libsystem_kernel.dylib        	       0x1862c8b9c semaphore_wait_trap + 8
1   libdispatch.dylib             	       0x18614e49c _dispatch_sema4_wait + 28
2   libdispatch.dylib             	       0x18614eb28 _dispatch_semaphore_wait_slow + 132
3   satcomp                       	      0x705551aa00 0x70553a4000 + 1534464
4   satcomp                       	      0x705540715c 0x70553a4000 + 405852
5   satcomp                       	      0x7055408be4 0x70553a4000 + 412644
6   satcomp                       	      0x70553dfb7c 0x70553a4000 + 244604
7   satcomp                       	      0x70553fc18c 0x70553a4000 + 360844
8   satcomp                       	      0x705540af8c 0x70553a4000 + 421772
9   satcomp                       	      0x70553c1824 0x70553a4000 + 120868
10  satcomp                       	      0x705551f188 0x70553a4000 + 1552776
11  libsystem_pthread.dylib       	       0x18630dd00 _pthread_start + 136
12  libsystem_pthread.dylib       	       0x186308cec thread_start + 8

Thread 11:

Thread 12:: com.apple.audio.IOThread.client
0   libsystem_kernel.dylib        	       0x1862c8ba8 semaphore_wait_signal_trap + 8
1   caulk                         	       0x193267fc0 caulk::mach::semaphore::wait_signal_or_error(caulk::mach::semaphore&) + 36
2   CoreAudio                     	       0x189bf52d8 HALC_ProxyIOContext::IOWorkLoop() + 4932
3   CoreAudio                     	       0x1897c9b6c invocation function for block in HALC_ProxyIOContext::HALC_ProxyIOContext(unsigned int, unsigned int) + 172
4   CoreAudio                     	       0x189996c74 HALC_IOThread::Entry(void*) + 92
5   libsystem_pthread.dylib       	       0x18630dd00 _pthread_start + 136
6   libsystem_pthread.dylib       	       0x186308cec thread_start + 8

Thread 13:: */mediaafx
0   libsystem_kernel.dylib        	       0x1862cc50c __psynch_cvwait + 8
1   libsystem_pthread.dylib       	       0x18630e1fc _pthread_cond_wait + 1024
2   REAPER                        	       0x103390950 WaitForSingleObject(void*, unsigned int) + 668
3   REAPER                        	       0x102f9c804 WorkerThreadInst::threadProc(void*) + 2284
4   REAPER                        	       0x103390b38 __threadproc(void*) + 32
5   libsystem_pthread.dylib       	       0x18630dd00 _pthread_start + 136
6   libsystem_pthread.dylib       	       0x186308cec thread_start + 8

Thread 14:: */mediaafx
0   libsystem_kernel.dylib        	       0x1862cc50c __psynch_cvwait + 8
1   libsystem_pthread.dylib       	       0x18630e1fc _pthread_cond_wait + 1024
2   REAPER                        	       0x103390950 WaitForSingleObject(void*, unsigned int) + 668
3   REAPER                        	       0x102f9c804 WorkerThreadInst::threadProc(void*) + 2284
4   REAPER                        	       0x103390b38 __threadproc(void*) + 32
5   libsystem_pthread.dylib       	       0x18630dd00 _pthread_start + 136
6   libsystem_pthread.dylib       	       0x186308cec thread_start + 8

Thread 15:: */mediaafx
0   libsystem_kernel.dylib        	       0x1862cc50c __psynch_cvwait + 8
1   libsystem_pthread.dylib       	       0x18630e1fc _pthread_cond_wait + 1024
2   REAPER                        	       0x103390950 WaitForSingleObject(void*, unsigned int) + 668
3   REAPER                        	       0x102f9c804 WorkerThreadInst::threadProc(void*) + 2284
4   REAPER                        	       0x103390b38 __threadproc(void*) + 32
5   libsystem_pthread.dylib       	       0x18630dd00 _pthread_start + 136
6   libsystem_pthread.dylib       	       0x186308cec thread_start + 8

Thread 16:: */mediaafx
0   libsystem_kernel.dylib        	       0x1862cc50c __psynch_cvwait + 8
1   libsystem_pthread.dylib       	       0x18630e1fc _pthread_cond_wait + 1024
2   REAPER                        	       0x103390950 WaitForSingleObject(void*, unsigned int) + 668
3   REAPER                        	       0x102f9c804 WorkerThreadInst::threadProc(void*) + 2284
4   REAPER                        	       0x103390b38 __threadproc(void*) + 32
5   libsystem_pthread.dylib       	       0x18630dd00 _pthread_start + 136
6   libsystem_pthread.dylib       	       0x186308cec thread_start + 8

Thread 17:: */mediaafx
0   libsystem_kernel.dylib        	       0x1862cc50c __psynch_cvwait + 8
1   libsystem_pthread.dylib       	       0x18630e1fc _pthread_cond_wait + 1024
2   REAPER                        	       0x103390950 WaitForSingleObject(void*, unsigned int) + 668
3   REAPER                        	       0x102f9c804 WorkerThreadInst::threadProc(void*) + 2284
4   REAPER                        	       0x103390b38 __threadproc(void*) + 32
5   libsystem_pthread.dylib       	       0x18630dd00 _pthread_start + 136
6   libsystem_pthread.dylib       	       0x186308cec thread_start + 8

Thread 18:: */mediaafx
0   libsystem_kernel.dylib        	       0x1862cc50c __psynch_cvwait + 8
1   libsystem_pthread.dylib       	       0x18630e1fc _pthread_cond_wait + 1024
2   REAPER                        	       0x103390950 WaitForSingleObject(void*, unsigned int) + 668
3   REAPER                        	       0x102f9c804 WorkerThreadInst::threadProc(void*) + 2284
4   REAPER                        	       0x103390b38 __threadproc(void*) + 32
5   libsystem_pthread.dylib       	       0x18630dd00 _pthread_start + 136
6   libsystem_pthread.dylib       	       0x186308cec thread_start + 8

Thread 19:: */mediaafx
0   libsystem_kernel.dylib        	       0x1862cc50c __psynch_cvwait + 8
1   libsystem_pthread.dylib       	       0x18630e1fc _pthread_cond_wait + 1024
2   REAPER                        	       0x103390950 WaitForSingleObject(void*, unsigned int) + 668
3   REAPER                        	       0x102f9c804 WorkerThreadInst::threadProc(void*) + 2284
4   REAPER                        	       0x103390b38 __threadproc(void*) + 32
5   libsystem_pthread.dylib       	       0x18630dd00 _pthread_start + 136
6   libsystem_pthread.dylib       	       0x186308cec thread_start + 8

Thread 20:: */mediaafx
0   libsystem_kernel.dylib        	       0x1862cc50c __psynch_cvwait + 8
1   libsystem_pthread.dylib       	       0x18630e1fc _pthread_cond_wait + 1024
2   REAPER                        	       0x103390950 WaitForSingleObject(void*, unsigned int) + 668
3   REAPER                        	       0x102f9c804 WorkerThreadInst::threadProc(void*) + 2284
4   REAPER                        	       0x103390b38 __threadproc(void*) + 32
5   libsystem_pthread.dylib       	       0x18630dd00 _pthread_start + 136
6   libsystem_pthread.dylib       	       0x186308cec thread_start + 8

Thread 21:: */mediaafx
0   libsystem_kernel.dylib        	       0x1862cc50c __psynch_cvwait + 8
1   libsystem_pthread.dylib       	       0x18630e1fc _pthread_cond_wait + 1024
2   REAPER                        	       0x103390950 WaitForSingleObject(void*, unsigned int) + 668
3   REAPER                        	       0x102f9c804 WorkerThreadInst::threadProc(void*) + 2284
4   REAPER                        	       0x103390b38 __threadproc(void*) + 32
5   libsystem_pthread.dylib       	       0x18630dd00 _pthread_start + 136
6   libsystem_pthread.dylib       	       0x186308cec thread_start + 8

Thread 22:: */mediaafx
0   libsystem_kernel.dylib        	       0x1862cc50c __psynch_cvwait + 8
1   libsystem_pthread.dylib       	       0x18630e1fc _pthread_cond_wait + 1024
2   REAPER                        	       0x103390950 WaitForSingleObject(void*, unsigned int) + 668
3   REAPER                        	       0x102f9c804 WorkerThreadInst::threadProc(void*) + 2284
4   REAPER                        	       0x103390b38 __threadproc(void*) + 32
5   libsystem_pthread.dylib       	       0x18630dd00 _pthread_start + 136
6   libsystem_pthread.dylib       	       0x186308cec thread_start + 8

Thread 23:: */livefx
0   libsystem_kernel.dylib        	       0x1862cc50c __psynch_cvwait + 8
1   libsystem_pthread.dylib       	       0x18630e1fc _pthread_cond_wait + 1024
2   REAPER                        	       0x103390950 WaitForSingleObject(void*, unsigned int) + 668
3   REAPER                        	       0x102f9cb3c WorkerThreadInst::threadProc(void*) + 3108
4   REAPER                        	       0x103390b38 __threadproc(void*) + 32
5   libsystem_pthread.dylib       	       0x18630dd00 _pthread_start + 136
6   libsystem_pthread.dylib       	       0x186308cec thread_start + 8

Thread 24:: */livefx
0   libsystem_kernel.dylib        	       0x1862cc50c __psynch_cvwait + 8
1   libsystem_pthread.dylib       	       0x18630e1fc _pthread_cond_wait + 1024
2   REAPER                        	       0x103390950 WaitForSingleObject(void*, unsigned int) + 668
3   REAPER                        	       0x102f9cb3c WorkerThreadInst::threadProc(void*) + 3108
4   REAPER                        	       0x103390b38 __threadproc(void*) + 32
5   libsystem_pthread.dylib       	       0x18630dd00 _pthread_start + 136
6   libsystem_pthread.dylib       	       0x186308cec thread_start + 8

Thread 25:: */livefx
0   libsystem_kernel.dylib        	       0x1862cc50c __psynch_cvwait + 8
1   libsystem_pthread.dylib       	       0x18630e1fc _pthread_cond_wait + 1024
2   REAPER                        	       0x103390950 WaitForSingleObject(void*, unsigned int) + 668
3   REAPER                        	       0x102f9cb3c WorkerThreadInst::threadProc(void*) + 3108
4   REAPER                        	       0x103390b38 __threadproc(void*) + 32
5   libsystem_pthread.dylib       	       0x18630dd00 _pthread_start + 136
6   libsystem_pthread.dylib       	       0x186308cec thread_start + 8

Thread 26:

Thread 27:

Thread 28:

Thread 29:

Thread 30:


Thread 0 crashed with ARM Thread State (64-bit):
    x0: 0x0000000000000000   x1: 0x0000000000000000   x2: 0x0000000000000000   x3: 0x0000000000000000
    x4: 0x000000013d34df90   x5: 0x0000000138518c00   x6: 0x0000000000000002   x7: 0x0000000000000000
    x8: 0x533b4bdd5ace8e81   x9: 0x533b4bdcb12df101  x10: 0x0000000000000002  x11: 0x00000000fffffffd
   x12: 0x0000000000000000  x13: 0x0000000000000000  x14: 0x0000000000000000  x15: 0x0000000000000000
   x16: 0x0000000000000148  x17: 0x00000001eda73518  x18: 0x0000000000000000  x19: 0x0000000000000006
   x20: 0x0000000000000103  x21: 0x00000001ebe38060  x22: 0x000000705577c518  x23: 0x0000007055784000
   x24: 0x0000007055782000  x25: 0x0000000000041c00  x26: 0x000002a800000176  x27: 0x000000016cf9bb28
   x28: 0x0000000000000000   fp: 0x000000016cf9b7e0   lr: 0x000000018630d97c
    sp: 0x000000016cf9b7c0   pc: 0x00000001862d1650 cpsr: 0x40000000
   far: 0x0000000000000000  esr: 0x56000080 (Syscall)

Binary Images:
       0x102e60000 -        0x103873fff com.cockos.reaper (7.76.0_6cb0d69u) <63af0b43-88d0-345e-80d7-5bdce53efce9> /Applications/REAPER.app/Contents/MacOS/REAPER
       0x103e64000 -        0x103e6ffff libobjc-trampolines.dylib (*) <9ef9e491-f3d3-322c-918a-559d3b858d9a> /usr/lib/libobjc-trampolines.dylib
       0x113ee4000 -        0x113efffff libMetalMetricsInterpose.dylib (*) <b7d8c7ba-8c30-3362-b3c7-d0998077dc1d> /Volumes/VOLUME/*/libMetalMetricsInterpose.dylib
       0x129ca4000 -        0x129cd3fff elastique3.dylib (*) <fdeee20a-415a-3c11-b3e0-2f7a3d2ffb81> /Applications/REAPER.app/Contents/Plugins/elastique3.dylib
       0x129c00000 -        0x129c07fff soundtouch.dylib (*) <b467a3bf-d8d5-30c0-a58e-945a0ce2c239> /Applications/REAPER.app/Contents/Plugins/soundtouch.dylib
       0x11af90000 -        0x11afabfff rubberband.dylib (*) <e8b80eeb-df32-3f3f-99d5-8c869d457618> /Applications/REAPER.app/Contents/Plugins/rubberband.dylib
       0x129c18000 -        0x129c6ffff jsfx.dylib (*) <1a896670-e67d-3d49-88dd-cabca854a415> /Applications/REAPER.app/Contents/Plugins/jsfx.dylib
       0x11ae20000 -        0x11ae43fff reaper_wave.dylib (*) <ba5355e0-01c0-31c8-827e-bc457175245a> /Applications/REAPER.app/Contents/Plugins/reaper_wave.dylib
       0x12b09c000 -        0x12b0c3fff reaper_mp3dec.dylib (*) <5c3d5154-798c-35da-bf71-d3b736a81df4> /Applications/REAPER.app/Contents/Plugins/reaper_mp3dec.dylib
       0x12b2bc000 -        0x12b303fff libmp3lame.dylib (*) <22134e04-d839-3b7c-a8ee-3aa135952a89> /Applications/REAPER.app/Contents/Plugins/libmp3lame.dylib
       0x12b654000 -        0x12b77ffff reaper_midi.dylib (*) <981f8b48-8c07-33ad-8fc4-0043cc789863> /Applications/REAPER.app/Contents/Plugins/reaper_midi.dylib
       0x12b348000 -        0x12b377fff reaper_wavpack.dylib (*) <3c5acd7b-deff-3364-a7ae-635d353222a6> /Applications/REAPER.app/Contents/Plugins/reaper_wavpack.dylib
       0x12b200000 -        0x12b26ffff reaper_opus.dylib (*) <13ce821c-8164-303c-9376-14ada1d65c12> /Applications/REAPER.app/Contents/Plugins/reaper_opus.dylib
       0x12b42c000 -        0x12b45bfff reaper_flac.dylib (*) <54e5c255-fa88-38e9-95ff-74c414b70927> /Applications/REAPER.app/Contents/Plugins/reaper_flac.dylib
       0x12b3e8000 -        0x12b3f7fff reaper_rex.dylib (*) <34e67550-7d19-3ba8-bcd3-6fd90b60e780> /Applications/REAPER.app/Contents/Plugins/reaper_rex.dylib
       0x11ae9c000 -        0x11aea7fff reaper_ddp.dylib (*) <f0c06206-3678-308d-986f-cc50f60ea061> /Applications/REAPER.app/Contents/Plugins/reaper_ddp.dylib
       0x12b55c000 -        0x12b5abfff reaper_explorer.dylib (*) <daed245b-073b-35bd-93a0-5be72df093d8> /Applications/REAPER.app/Contents/Plugins/reaper_explorer.dylib
       0x12b290000 -        0x12b29bfff reaper_cd.dylib (*) <056b3fca-d3df-34ad-8a18-97ec0982aa79> /Applications/REAPER.app/Contents/Plugins/reaper_cd.dylib
       0x12c1ec000 -        0x12c293fff reaper_ogg.dylib (*) <4328fdf4-6cff-3b59-8602-cdb8a829d878> /Applications/REAPER.app/Contents/Plugins/reaper_ogg.dylib
       0x12b5d0000 -        0x12b60ffff reaper_csurf.dylib (*) <beba3477-3808-3dec-ba8a-cddff1b94f65> /Applications/REAPER.app/Contents/Plugins/reaper_csurf.dylib
       0x12c000000 -        0x12c0f7fff reaper_video.dylib (*) <f48aab88-2228-3f9f-946c-58e30e57428f> /Applications/REAPER.app/Contents/Plugins/reaper_video.dylib
       0x12b3c8000 -        0x12b3cffff reap_osx_modern.dylib (*) <30492a04-5521-3661-8c24-5cf77c5b71ac> /Applications/REAPER.app/Contents/Plugins/reap_osx_modern.dylib
       0x12c2e0000 -        0x12c353fff libavutil.60.26.101.dylib (*) <c65071ae-5ed4-3e54-b56c-4b625005edbc> /opt/homebrew/*/libavutil.60.26.101.dylib
       0x12c48c000 -        0x12c51bfff libssl.3.dylib (*) <704bf8e6-0dec-398e-986f-2eb58f3c6b3c> /opt/homebrew/*/libssl.3.dylib
       0x12ca04000 -        0x12cd43fff libcrypto.3.dylib (*) <6d7e86a1-ba7d-3b61-bfc7-248b1221632e> /opt/homebrew/*/libcrypto.3.dylib
       0x12c564000 -        0x12c5e3fff libswscale.9.5.101.dylib (*) <50bfe305-8558-335e-b305-09019ce57377> /opt/homebrew/*/libswscale.9.5.101.dylib
       0x12b3ac000 -        0x12b3bbfff libswresample.6.3.101.dylib (*) <e59853f8-fd79-3cc8-95d6-95ff184ba81f> /opt/homebrew/*/libswresample.6.3.101.dylib
       0x12d7d8000 -        0x12e087fff libavcodec.62.28.101.dylib (*) <49303757-5681-368c-9db2-af5b5218adb6> /opt/homebrew/*/libavcodec.62.28.101.dylib
       0x12c7ec000 -        0x12c963fff libvpx.12.dylib (*) <abef3286-0a7b-3e10-8825-81c8a863efa8> /opt/homebrew/*/libvpx.12.dylib
       0x12c63c000 -        0x12c6d3fff libdav1d.7.dylib (*) <b317db1b-7e5e-3ad6-8dfb-5f30139ba348> /opt/homebrew/*/libdav1d.7.dylib
       0x12b4c4000 -        0x12b4f7fff libmp3lame.0.dylib (*) <de863ab7-3cf8-38fe-ae37-59a526573730> /opt/homebrew/*/libmp3lame.0.dylib
       0x12c738000 -        0x12c783fff libopus.0.dylib (*) <1c7445f6-fb0e-33ea-8aa8-18597a9443ac> /opt/homebrew/*/libopus.0.dylib
       0x12d168000 -        0x12d3dbfff libSvtAv1Enc.4.1.0.dylib (*) <06e58860-87c7-3851-bb90-8d08a10ae2e7> /opt/homebrew/*/libSvtAv1Enc.4.1.0.dylib
       0x12d468000 -        0x12d587fff libx264.165.dylib (*) <b916484f-5239-335b-b9fe-4e71261da16b> /opt/homebrew/*/libx264.165.dylib
       0x12eed0000 -        0x12f4affff libx265.216.dylib (*) <710dc56c-1d6c-3b7b-bba6-157396ddc818> /opt/homebrew/*/libx265.216.dylib
       0x12e7ac000 -        0x12e997fff libavformat.62.12.101.dylib (*) <da948b54-d8e2-3e3d-8efc-097ee2fbc2a7> /opt/homebrew/*/libavformat.62.12.101.dylib
       0x12cf88000 -        0x12d04bfff libvlccore.9.dylib (*) <600321d1-62bd-395f-8699-aed018555ae1> /Applications/VLC.app/Contents/MacOS/lib/libvlccore.9.dylib
       0x16d630000 -        0x16e307fff libavcodec_plugin.dylib (*) <3da6d39e-ac45-3e06-acd0-b66c162cc860> /Applications/VLC.app/Contents/MacOS/plugins/libavcodec_plugin.dylib
       0x12b7cc000 -        0x12b7e3fff libvlc.5.dylib (*) <60f2f0e6-8acf-3f55-859c-17546d85ac58> /Applications/VLC.app/Contents/MacOS/lib/libvlc.5.dylib
       0x12b478000 -        0x12b47ffff libi420_rgb_plugin.dylib (*) <e5ee2c91-b180-3082-9fec-f624c83e63af> /Applications/VLC.app/Contents/MacOS/plugins/libi420_rgb_plugin.dylib
       0x12b418000 -        0x12b41bfff libi420_10_p010_plugin.dylib (*) <f3610263-f7e8-31cb-9d9d-8eced5725499> /Applications/VLC.app/Contents/MacOS/plugins/libi420_10_p010_plugin.dylib
       0x12b490000 -        0x12b493fff libscaletempo_pitch_plugin.dylib (*) <9752c99b-c2a5-3a9f-a9e3-e14bfe735860> /Applications/VLC.app/Contents/MacOS/plugins/libscaletempo_pitch_plugin.dylib
       0x12b4a4000 -        0x12b4a7fff libi420_yuy2_plugin.dylib (*) <858f1447-7e20-3047-9a05-b313704bd9c1> /Applications/VLC.app/Contents/MacOS/plugins/libi420_yuy2_plugin.dylib
       0x12b53c000 -        0x12b53ffff libi422_yuy2_plugin.dylib (*) <ca4cf604-6588-39b3-91e7-9b1d5a2677a5> /Applications/VLC.app/Contents/MacOS/plugins/libi422_yuy2_plugin.dylib
       0x12b634000 -        0x12b637fff libyuy2_i420_plugin.dylib (*) <2dae5195-de46-32ef-9929-3f0dee40f299> /Applications/VLC.app/Contents/MacOS/plugins/libyuy2_i420_plugin.dylib
       0x12c148000 -        0x12c14ffff libi420_nv12_plugin.dylib (*) <d107acae-a835-3cc4-bd43-a200af1b8c24> /Applications/VLC.app/Contents/MacOS/plugins/libi420_nv12_plugin.dylib
       0x12c160000 -        0x12c163fff libyuy2_i422_plugin.dylib (*) <a0b9902a-a6e1-3cf7-bc76-a9997640a925> /Applications/VLC.app/Contents/MacOS/plugins/libyuy2_i422_plugin.dylib
       0x12c174000 -        0x12c17bfff libkeychain_plugin.dylib (*) <3c6552be-64b8-3221-9232-985d8666f08b> /Applications/VLC.app/Contents/MacOS/plugins/libkeychain_plugin.dylib
       0x12c18c000 -        0x12c18ffff libi422_i420_plugin.dylib (*) <4279056d-c9e3-3961-beab-8b886b9d18d3> /Applications/VLC.app/Contents/MacOS/plugins/libi422_i420_plugin.dylib
       0x12c1a0000 -        0x12c1a3fff libgrey_yuv_plugin.dylib (*) <d57407c8-8e20-3672-a170-48b1af424fb8> /Applications/VLC.app/Contents/MacOS/plugins/libgrey_yuv_plugin.dylib
       0x12c1b4000 -        0x12c1b7fff libchain_plugin.dylib (*) <11fe731b-8256-3bff-9f23-c3e40e4db6b0> /Applications/VLC.app/Contents/MacOS/plugins/libchain_plugin.dylib
       0x12c1c8000 -        0x12c1cbfff libscale_plugin.dylib (*) <482bb48a-a07d-3dbc-89a7-0d594e17796d> /Applications/VLC.app/Contents/MacOS/plugins/libscale_plugin.dylib
       0x12c794000 -        0x12c797fff libyuvp_plugin.dylib (*) <fac3d456-4c92-372a-b223-14d1969c13cc> /Applications/VLC.app/Contents/MacOS/plugins/libyuvp_plugin.dylib
       0x12d698000 -        0x12d75ffff libdav1d_plugin.dylib (*) <81f4eb4e-87ff-3445-801c-2ecc40351645> /Applications/VLC.app/Contents/MacOS/plugins/libdav1d_plugin.dylib
       0x12c7a8000 -        0x12c7abfff libyuv_plugin.dylib (*) <6446a71d-d909-3354-960f-27c7efabc9a6> /Applications/VLC.app/Contents/MacOS/plugins/libyuv_plugin.dylib
       0x12c7bc000 -        0x12c7bffff libscaletempo_plugin.dylib (*) <23f21c98-24f3-3a7e-a975-ed7fc87097e8> /Applications/VLC.app/Contents/MacOS/plugins/libscaletempo_plugin.dylib
       0x12e9ec000 -        0x12ea93fff libswscale_plugin.dylib (*) <6d9def6a-e09f-3d0b-9a39-487a972b513b> /Applications/VLC.app/Contents/MacOS/plugins/libswscale_plugin.dylib
       0x12f608000 -        0x12f87bfff reaper_imgui-arm64.dylib (*) <c7798dc2-75c9-38fe-b9a4-235e2091f679> /Users/USER/Library/Application Support/REAPER/*/reaper_imgui-arm64.dylib
       0x12ec8c000 -        0x12edabfff reaper_js_ReaScriptAPI64ARM.dylib (*) <1ec74161-eae1-325c-bab0-3eec253a12a7> /Users/USER/Library/Application Support/REAPER/*/reaper_js_ReaScriptAPI64ARM.dylib
       0x12fb84000 -        0x12fd27fff reaper_reapack-arm64.dylib (*) <99d2acf4-bfe6-3926-8582-d8e8688cafb4> /Users/USER/Library/Application Support/REAPER/*/reaper_reapack-arm64.dylib
       0x168000000 -        0x16840bfff reaper_sws-arm64.dylib (*) <5ae5b818-2553-3924-89df-94afbfb7c922> /Library/Application Support/REAPER/*/reaper_sws-arm64.dylib
       0x13979c000 -        0x1397bbfff com.meldaproduction.MStereoScope (*) <995af32c-dc8d-349e-ab7d-f47c18f2666d> /Library/Audio/Plug-Ins/VST3/MeldaProduction/Stereo/MStereoScope.vst3/Contents/MacOS/MStereoScope.bin
      0x7016f60000 -       0x7019b53fff libMeldaProductionAudioPluginKernelV11.dylib (*) <2f2f4952-9d06-330c-9cc5-04ac8a6a1f5f> /Library/Application Support/MeldaProduction/*/libMeldaProductionAudioPluginKernelV11.dylib
      0x7010740000 -       0x7010e87fff com.voxengo.audio-plugins.VST3.SPAN (3.23) <3aa1dc06-f953-34a7-82d9-9fb75dc2b173> /Library/Audio/Plug-Ins/VST3/SPAN.vst3/Contents/MacOS/SPAN
      0x7010ed8000 -       0x70114fbfff com.voxengo.audio-plugins.VST3.Correlometer (1.9) <056e4443-5c3e-33a4-b988-b4e1aefafe48> /Library/Audio/Plug-Ins/VST3/Correlometer.vst3/Contents/MacOS/Correlometer
      0x701a680000 -       0x701ade7fff com.Youlean.vst3.Youlean-Loudness-Meter-2 (2.5.12) <1591aaa9-1e2b-36d7-b3e6-4436ae564f01> /Library/Audio/Plug-Ins/VST3/Youlean Loudness Meter 2.vst3/Contents/MacOS/Youlean Loudness Meter 2
      0x701001c000 -       0x70103e3fff com.DsgDnB.Scyllascope (1.3.0) <5c5a7267-7020-3aad-8619-b88f2444540a> /Library/Audio/Plug-Ins/VST3/Scyllascope.vst3/Contents/MacOS/Scyllascope
      0x70553a4000 -       0x7055773fff com.nih-plug.satcomp (1.0.0) <077b5954-300c-32ec-95c5-4de9f1b60db5> /Library/Audio/Plug-Ins/VST3/satcomp.vst3/Contents/MacOS/satcomp
      0x700dc94000 -       0x700dcf7fff com.apple.AppleMetalOpenGLRenderer (1.0) <140d0cef-f342-380f-b7e7-cad89a979ec2> /System/Library/Extensions/AppleMetalOpenGLRenderer.bundle/Contents/MacOS/AppleMetalOpenGLRenderer
       0x1862c8000 -        0x1863069ab libsystem_kernel.dylib (*) <f63bf418-8f75-34a4-aaf1-8caf2b9b6a05> /usr/lib/system/libsystem_kernel.dylib
       0x186307000 -        0x18631391b libsystem_pthread.dylib (*) <5a1828c3-cbc2-3b85-bd07-c8ed10021a41> /usr/lib/system/libsystem_pthread.dylib
       0x186195000 -        0x186217ca3 libsystem_c.dylib (*) <fba7b23e-aa60-3a90-9aa4-a7a2e0ad63ee> /usr/lib/system/libsystem_c.dylib
       0x18634d000 -        0x1867dde9f com.apple.CoreFoundation (6.9) <727e905e-f344-3823-8e13-e7a096388321> /System/Library/Frameworks/CoreFoundation.framework/Versions/A/CoreFoundation
       0x187a77000 -        0x188d34287 com.apple.Foundation (6.9) <4317f7c6-73d2-3be9-b010-f318f1198a1b> /System/Library/Frameworks/Foundation.framework/Versions/C/Foundation
       0x18a94b000 -        0x18bf8d50b com.apple.AppKit (6.9) <2688afbc-5c39-3132-bcf5-8d00d32d7615> /System/Library/Frameworks/AppKit.framework/Versions/C/AppKit
       0x193745000 -        0x193a294ff com.apple.HIToolbox (2.1.1) <5d739a34-36b7-39b9-ba3b-7608b59c07a8> /System/Library/Frameworks/Carbon.framework/Versions/A/Frameworks/HIToolbox.framework/Versions/A/HIToolbox
       0x185f06000 -        0x185fc56e3 dyld (*) <d341ddcb-7969-3a9c-8e03-0cf4751398a9> /usr/lib/dyld
               0x0 - 0xffffffffffffffff ??? (*) <00000000-0000-0000-0000-000000000000> ???
       0x1a7ca2000 -        0x1a7d68353 com.apple.audio.midi.CoreMIDI (2.0) <da3e2cf1-d70c-3055-bf59-0f604da694a9> /System/Library/Frameworks/CoreMIDI.framework/Versions/A/CoreMIDI
       0x186218000 -        0x1862ad033 libc++.1.dylib (*) <1e9ba4aa-b5da-3cf3-80ac-4b880049573f> /usr/lib/libc++.1.dylib
       0x18614b000 -        0x18619119f libdispatch.dylib (*) <022bf08f-0b3a-3225-9e65-16d1553ce90d> /usr/lib/system/libdispatch.dylib
       0x193248000 -        0x193272053 com.apple.audio.caulk (1.0) <c7ba2b3e-b175-30e9-abf6-4d568d9daa51> /System/Library/PrivateFrameworks/caulk.framework/Versions/A/caulk
       0x1895e5000 -        0x189d82633 com.apple.audio.CoreAudio (5.0) <3ab8f076-7437-38aa-9f5b-b083cf020ada> /System/Library/Frameworks/CoreAudio.framework/Versions/A/CoreAudio

External Modification Summary:
  Calls made by other processes targeting this process:
    task_for_pid: 0
    thread_create: 0
    thread_set_state: 0
  Calls made by this process:
    task_for_pid: 0
    thread_create: 0
    thread_set_state: 0
  Calls made by all processes on this machine:
    task_for_pid: 0
    thread_create: 0
    thread_set_state: 0

VM Region Summary:
ReadOnly portion of Libraries: Total=3.1G resident=0K(0%) swapped_out_or_unallocated=3.1G(100%)
Writable regions: Total=2.9G written=2547K(0%) resident=1731K(0%) swapped_out=816K(0%) unallocated=2.9G(100%)

                                VIRTUAL   REGION 
REGION TYPE                        SIZE    COUNT (non-coalesced) 
===========                     =======  ======= 
Accelerate Framework               384K        3 
Activity Tracing                   256K        1 
AttributeGraph Data               1024K        1 
CG Image                           224K       12 
ColorSync                           64K        4 
CoreAnimation                     5088K      194 
CoreGraphics                        64K        4 
CoreImage                           32K        2 
CoreServices                       176K        1 
CoreUI Image Data                 3504K       31 
Dyld Private Memory                128K        1 
Foundation                          48K        2 
Guard                            385.0G        1 
Image IO                           256K       16 
Mach Message                       400K        3 
Malloc                             2.6G      164 
Malloc Guard Page                  480K       30 
Malloc Large (reserved)          213.2M        3         reserved VM address space (unallocated)
OS Alloc Once                       32K        1 
OpenGL GLSL                        256K        3 
Stack                             25.4M       31 
Stack Guard                       56.5M       31 
Untagged                          2144K       43 
Untagged (reserved)                768K        1         reserved VM address space (unallocated)
__AUTH                            6690K      713 
__AUTH_CONST                     100.0M     1067 
__CTF                               824        1 
__DATA                            99.0M     1165 
__DATA_CONST                      42.2M     1138 
__DATA_DIRTY                      14.7M      952 
__FONT_DATA                        2352        1 
__GLSLBUILTINS                    5176K        1 
__LINKEDIT                         1.6G       70 
__OBJC_RO                        118.0M        1 
__OBJC_RW                         2735K        1 
__TEXT                             1.5G     1227 
__TPRO_CONST                       132K        4 
mapped file                      528.5M       66 
page table in kernel              1731K        1 
shared memory                     1040K       25 
===========                     =======  ======= 
TOTAL                            391.9G     7016 
TOTAL, minus reserved VM space   391.7G     7016 


-----------
Full Report
-----------

{"app_name":"REAPER","timestamp":"2026-11-09 12:23:12.00 +0700","app_version":"7.76.0_6cb0d69u","slice_uuid":"63af0b43-88d0-345e-80d7-5bdce53efce9","build_version":"7.76.0_6cb0d69u","platform":1,"bundleID":"com.cockos.reaper","share_with_app_devs":1,"is_first_party":0,"bug_type":"309","os_version":"macOS 27.0.1 (26A434)","roots_installed":0,"name":"REAPER","incident_id":"9A940F2A-0125-45AD-9DFD-3E4A04624005"}
{
  "uptime" : 680000,
  "procRole" : "Foreground",
  "version" : 2,
  "userID" : 501,
  "deployVersion" : 210,
  "modelCode" : "Mac16,1",
  "coalitionID" : 2349,
  "osVersion" : {
    "train" : "macOS 27.0.1",
    "build" : "26A434",
    "releaseType" : "User"
  },
  "captureTime" : "2026-11-09 12:23:02.5650 +0700",
  "codeSigningMonitor" : 2,
  "incident" : "9A940F2A-0125-45AD-9DFD-3E4A04624005",
  "pid" : 64971,
  "translated" : false,
  "cpuType" : "ARM-64",
  "procLaunch" : "2026-11-09 12:16:28.9903 +0700",
  "procStartAbsTime" : 16487735686715,
  "procExitAbsTime" : 16497174125932,
  "procName" : "REAPER",
  "procPath" : "\/Applications\/REAPER.app\/Contents\/MacOS\/REAPER",
  "bundleInfo" : {"CFBundleShortVersionString":"7.76.0_6cb0d69u","CFBundleVersion":"7.76.0_6cb0d69u","CFBundleIdentifier":"com.cockos.reaper"},
  "storeInfo" : {"deviceIdentifierForVendor":"33852C0A-DBC5-520A-BD13-E89C78348C42","thirdParty":true},
  "parentProc" : "launchd",
  "parentPid" : 1,
  "coalitionName" : "com.cockos.reaper",
  "crashReporterKey" : "BCAEB6EC-392E-1692-8605-C8DE47F2E63E",
  "chargingStatus" : {"externalSourceConnected":true},
  "developerMode" : 1,
  "codeSigningID" : "com.cockos.reaper",
  "codeSigningTeamID" : "Y3T58622SG",
  "codeSigningFlags" : 570491653,
  "codeSigningValidationCategory" : 6,
  "codeSigningTrustLevel" : 4294967295,
  "codeSigningAuxiliaryInfo" : 0,
  "instructionByteStream" : {"beforePC":"fyMD1f17v6n9AwCR69\/\/l78DAJH9e8Go\/w9f1sADX9YQKYDSARAA1A==","atPC":"AwEAVH8jA9X9e7+p\/QMAkeDf\/5e\/AwCR\/XvBqP8PX9bAA1\/WcAqA0g=="},
  "bootSessionUUID" : "155FC183-A78F-4EF9-97A3-47237D7C69EF",
  "wakeTime" : 4922,
  "sleepWakeUUID" : "A67ADBBD-58F8-4755-A606-8BA2EECBD9F6",
  "sip" : "enabled",
  "exception" : {"codes":"0x0000000000000000, 0x0000000000000000","rawCodes":[0,0],"type":"EXC_CRASH","signal":"SIGABRT"},
  "termination" : {"flags":0,"code":6,"namespace":"SIGNAL","indicator":"Abort trap: 6","byProc":"REAPER","byPid":64971},
  "asi" : {"libsystem_c.dylib":["abort() called"]},
  "extMods" : {"caller":{"thread_create":0,"thread_set_state":0,"task_for_pid":0},"system":{"thread_create":0,"thread_set_state":0,"task_for_pid":0},"targeted":{"thread_create":0,"thread_set_state":0,"task_for_pid":0},"warnings":0},
  "faultingThread" : 0,
  "threads" : [{"threadState":{"x":[{"value":0},{"value":0},{"value":0},{"value":0},{"value":5321842576},{"value":5239835648},{"value":2},{"value":0},{"value":5997470742857289345},{"value":5997470740011413761},{"value":2},{"value":4294967293},{"value":0},{"value":0},{"value":0},{"value":0},{"value":328},{"value":8282125592},{"value":0},{"value":6},{"value":259},{"value":8252522592,"symbolLocation":224,"symbol":"_main_thread"},{"value":482470249752,"symbolLocation":20568,"symbol":"clap_entry"},{"value":482470281216},{"value":482470273024},{"value":269312},{"value":2920577761654},{"value":6123273000},{"value":0}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6546315644},"cpsr":{"value":1073741824},"fp":{"value":6123272160},"sp":{"value":6123272128},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546069072,"matchesCrashFrame":1},"far":{"value":0}},"id":9347784,"triggered":true,"name":"reaper","queue":"com.apple.main-thread","frames":[{"imageOffset":38480,"symbol":"__pthread_kill","symbolLocation":8,"imageIndex":69},{"imageOffset":27004,"symbol":"pthread_kill","symbolLocation":296,"imageIndex":70},{"imageOffset":504920,"symbol":"abort","symbolLocation":148,"imageIndex":71},{"imageOffset":1799068,"imageIndex":67},{"imageOffset":1798572,"imageIndex":67},{"imageOffset":1531148,"imageIndex":67},{"imageOffset":1446256,"imageIndex":67},{"imageOffset":1414728,"imageIndex":67},{"imageOffset":1448144,"imageIndex":67},{"imageOffset":1801176,"imageIndex":67},{"imageOffset":1800996,"imageIndex":67},{"imageOffset":1801396,"imageIndex":67},{"imageOffset":558696,"imageIndex":67},{"imageOffset":483744,"symbol":"__CFNOTIFICATIONCENTER_IS_CALLING_OUT_TO_AN_OBSERVER__","symbolLocation":148,"imageIndex":72},{"imageOffset":1069972,"symbol":"___CFXRegistrationPost_block_invoke","symbolLocation":92,"imageIndex":72},{"imageOffset":1069784,"symbol":"_CFXRegistrationPost","symbolLocation":440,"imageIndex":72},{"imageOffset":271312,"symbol":"_CFXNotificationPost","symbolLocation":688,"imageIndex":72},{"imageOffset":37772,"symbol":"-[NSNotificationCenter postNotificationName:object:userInfo:]","symbolLocation":88,"imageIndex":73},{"imageOffset":1626828,"symbol":"-[NSWindow resignKeyWindow]","symbolLocation":636,"imageIndex":74},{"imageOffset":5258228,"symbol":"-[REAPERSwell_modelesswindow resignKeyWindow]","symbolLocation":52,"imageIndex":0},{"imageOffset":10995264,"symbol":"-[NSWindow _sendResignKeyWindow]","symbolLocation":100,"imageIndex":74},{"imageOffset":1251968,"symbol":"-[NSWindow _changeKeyAndMainLimitedOK:]","symbolLocation":612,"imageIndex":74},{"imageOffset":1251316,"symbol":"-[NSWindow makeKeyWindow]","symbolLocation":144,"imageIndex":74},{"imageOffset":1250732,"symbol":"-[NSWindow _makeKeyRegardlessOfVisibility]","symbolLocation":56,"imageIndex":74},{"imageOffset":2507720,"symbol":"-[NSApplication _orderFrontModalWindow:relativeToWindow:]","symbolLocation":508,"imageIndex":74},{"imageOffset":2506736,"symbol":"-[NSApplication _commonBeginModalSessionForWindow:relativeToWindow:modalDelegate:didEndSelector:contextInfo:]","symbolLocation":1368,"imageIndex":74},{"imageOffset":2505232,"symbol":"__35-[NSApplication runModalForWindow:]_block_invoke_2","symbolLocation":36,"imageIndex":74},{"imageOffset":2505168,"symbol":"__35-[NSApplication runModalForWindow:]_block_invoke","symbolLocation":108,"imageIndex":74},{"imageOffset":2503296,"symbol":"_NSTryRunModal","symbolLocation":100,"imageIndex":74},{"imageOffset":2503004,"symbol":"-[NSApplication runModalForWindow:]","symbolLocation":276,"imageIndex":74},{"imageOffset":12992236,"symbol":"-[NSSavePanel runModal]","symbolLocation":336,"imageIndex":74},{"imageOffset":487612,"imageIndex":67},{"imageOffset":477760,"imageIndex":67},{"imageOffset":275416,"imageIndex":67},{"imageOffset":67408,"imageIndex":67},{"imageOffset":338752,"imageIndex":67},{"imageOffset":473996,"imageIndex":67},{"imageOffset":553136,"imageIndex":67},{"imageOffset":637132,"symbol":"__CFRUNLOOP_IS_CALLING_OUT_TO_A_TIMER_CALLBACK_FUNCTION__","symbolLocation":32,"imageIndex":72},{"imageOffset":636360,"symbol":"__CFRunLoopDoTimer","symbolLocation":1084,"imageIndex":72},{"imageOffset":635096,"symbol":"__CFRunLoopDoTimers","symbolLocation":280,"imageIndex":72},{"imageOffset":525788,"symbol":"__CFRunLoopRun","symbolLocation":1852,"imageIndex":72},{"imageOffset":1419308,"symbol":"_CFRunLoopRunSpecificWithOptions","symbolLocation":536,"imageIndex":72},{"imageOffset":767376,"symbol":"RunCurrentEventLoopInMode","symbolLocation":320,"imageIndex":75},{"imageOffset":780572,"symbol":"ReceiveNextEventCommon","symbolLocation":492,"imageIndex":75},{"imageOffset":2364288,"symbol":"_BlockUntilNextEventMatchingListInMode","symbolLocation":48,"imageIndex":75},{"imageOffset":3654300,"symbol":"_DPSBlockUntilNextEventMatchingListInMode","symbolLocation":292,"imageIndex":74},{"imageOffset":234748,"symbol":"_DPSNextEvent","symbolLocation":580,"imageIndex":74},{"imageOffset":9115464,"symbol":"-[NSApplication(NSEventRouting) _nextEventMatchingEventMask:untilDate:inMode:dequeue:]","symbolLocation":780,"imageIndex":74},{"imageOffset":9114220,"symbol":"-[NSApplication(NSEventRouting) nextEventMatchingMask:untilDate:inMode:dequeue:]","symbolLocation":292,"imageIndex":74},{"imageOffset":182540,"symbol":"-[NSApplication run]","symbolLocation":396,"imageIndex":74},{"imageOffset":19532,"symbol":"NSApplicationMain","symbolLocation":880,"imageIndex":74},{"imageOffset":204416,"symbol":"start","symbolLocation":6688,"imageIndex":76}]},{"id":9347795,"name":"com.apple.NSEventThread","threadState":{"x":[{"value":268451845},{"value":21592279046},{"value":8589934592,"objc-selector":""},{"value":100068443029504},{"value":0},{"value":100068443029504},{"value":2},{"value":4294967295},{"value":0},{"value":17179869184},{"value":0},{"value":2},{"value":0},{"value":0},{"value":23299},{"value":0},{"value":18446744073709551569},{"value":8282127384},{"value":0},{"value":4294967295},{"value":2},{"value":100068443029504},{"value":0},{"value":100068443029504},{"value":21592279046},{"value":6126133400},{"value":8589934592,"objc-selector":""},{"value":18446744073709550527},{"value":4412409862}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6546110688},"cpsr":{"value":0},"fp":{"value":6126133248},"sp":{"value":6126133168},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546033696},"far":{"value":0}},"frames":[{"imageOffset":3104,"symbol":"mach_msg2_trap","symbolLocation":8,"imageIndex":69},{"imageOffset":80096,"symbol":"mach_msg2_internal","symbolLocation":76,"imageIndex":69},{"imageOffset":39464,"symbol":"mach_msg_overwrite","symbolLocation":480,"imageIndex":69},{"imageOffset":4012,"symbol":"mach_msg","symbolLocation":24,"imageIndex":69},{"imageOffset":531180,"symbol":"__CFRunLoopServiceMachPort","symbolLocation":160,"imageIndex":72},{"imageOffset":525108,"symbol":"__CFRunLoopRun","symbolLocation":1172,"imageIndex":72},{"imageOffset":1419308,"symbol":"_CFRunLoopRunSpecificWithOptions","symbolLocation":536,"imageIndex":72},{"imageOffset":1431620,"symbol":"_NSEventThread","symbolLocation":184,"imageIndex":74},{"imageOffset":27904,"symbol":"_pthread_start","symbolLocation":136,"imageIndex":70},{"imageOffset":7404,"symbol":"thread_start","symbolLocation":8,"imageIndex":70}]},{"id":9347802,"frames":[],"threadState":{"x":[{"value":6127857664},{"value":47875},{"value":6127321088},{"value":0},{"value":409604},{"value":18446744073709551615},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0}],"flavor":"ARM_THREAD_STATE64","lr":{"value":0},"cpsr":{"value":0},"fp":{"value":0},"sp":{"value":6127857664},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546296024},"far":{"value":0}}},{"id":9347828,"frames":[{"imageOffset":3104,"symbol":"mach_msg2_trap","symbolLocation":8,"imageIndex":69},{"imageOffset":80096,"symbol":"mach_msg2_internal","symbolLocation":76,"imageIndex":69},{"imageOffset":39464,"symbol":"mach_msg_overwrite","symbolLocation":480,"imageIndex":69},{"imageOffset":4012,"symbol":"mach_msg","symbolLocation":24,"imageIndex":69},{"imageOffset":291112,"symbol":"XServerMachPort::ReceiveMessage(int&, void*, int&)","symbolLocation":104,"imageIndex":78},{"imageOffset":429556,"symbol":"MIDIProcess::MIDIInPortThread::Run()","symbolLocation":148,"imageIndex":78},{"imageOffset":378068,"symbol":"CADeprecated::XThread::RunHelper(void*)","symbolLocation":40,"imageIndex":78},{"imageOffset":425492,"symbol":"CADeprecated::CAPThread::Entry(void*)","symbolLocation":96,"imageIndex":78},{"imageOffset":27904,"symbol":"_pthread_start","symbolLocation":136,"imageIndex":70},{"imageOffset":7404,"symbol":"thread_start","symbolLocation":8,"imageIndex":70}],"threadState":{"x":[{"value":268451845},{"value":17179869186},{"value":0},{"value":339761977884672},{"value":0},{"value":339761977884672},{"value":100},{"value":0},{"value":0},{"value":17179869184},{"value":100},{"value":0},{"value":0},{"value":0},{"value":79107},{"value":0},{"value":18446744073709551569},{"value":8282127384},{"value":0},{"value":0},{"value":100},{"value":339761977884672},{"value":0},{"value":339761977884672},{"value":17179869186},{"value":6130149844},{"value":0},{"value":18446744073709550527},{"value":2}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6546110688},"cpsr":{"value":0},"fp":{"value":6130149504},"sp":{"value":6130149424},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546033696},"far":{"value":0}}},{"id":9347856,"name":"*\/video","threadState":{"x":[{"value":4},{"value":0},{"value":1},{"value":1},{"value":0},{"value":50000000},{"value":111},{"value":3440},{"value":8252564424,"symbolLocation":0,"symbol":"clock_sem"},{"value":3},{"value":17},{"value":6197075568528807424},{"value":1},{"value":16},{"value":6153399936},{"value":8252647208,"symbolLocation":0,"symbol":"OBJC_CLASS_$___NSCFString"},{"value":334},{"value":8282125640},{"value":0},{"value":0},{"value":6153400000},{"value":5034242048,"symbolLocation":0,"symbol":"SWELL_GetListViewHeaderHeight"},{"value":5034242048,"symbolLocation":0,"symbol":"SWELL_GetListViewHeaderHeight"},{"value":5034242048,"symbolLocation":0,"symbol":"SWELL_GetListViewHeaderHeight"},{"value":5034242048,"symbolLocation":0,"symbol":"SWELL_GetListViewHeaderHeight"},{"value":0},{"value":5034242048,"symbolLocation":0,"symbol":"SWELL_GetListViewHeaderHeight"},{"value":0},{"value":5034242048,"symbolLocation":0,"symbol":"SWELL_GetListViewHeaderHeight"}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6544830548},"cpsr":{"value":2684354560},"fp":{"value":6153399984},"sp":{"value":6153399936},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546047752},"far":{"value":0}},"frames":[{"imageOffset":17160,"symbol":"__semwait_signal","symbolLocation":8,"imageIndex":69},{"imageOffset":57428,"symbol":"nanosleep","symbolLocation":220,"imageIndex":71},{"imageOffset":57196,"symbol":"usleep","symbolLocation":68,"imageIndex":71},{"imageOffset":98092,"symbol":"videoDecodeThread(void*)","symbolLocation":1300,"imageIndex":20},{"imageOffset":5442360,"symbol":"__threadproc(void*)","symbolLocation":32,"imageIndex":0},{"imageOffset":27904,"symbol":"_pthread_start","symbolLocation":136,"imageIndex":70},{"imageOffset":7404,"symbol":"thread_start","symbolLocation":8,"imageIndex":70}]},{"id":9347899,"frames":[{"imageOffset":17676,"symbol":"__psynch_cvwait","symbolLocation":8,"imageIndex":69},{"imageOffset":29136,"symbol":"_pthread_cond_wait","symbolLocation":980,"imageIndex":70},{"imageOffset":26556,"symbol":"std::__1::condition_variable::wait(std::__1::unique_lock<std::__1::mutex>&)","symbolLocation":32,"imageIndex":79},{"imageOffset":260904,"imageIndex":65},{"imageOffset":260560,"imageIndex":65},{"imageOffset":27904,"symbol":"_pthread_start","symbolLocation":136,"imageIndex":70},{"imageOffset":7404,"symbol":"thread_start","symbolLocation":8,"imageIndex":70}],"threadState":{"x":[{"value":260},{"value":0},{"value":0},{"value":0},{"value":0},{"value":160},{"value":0},{"value":0},{"value":481295691368},{"value":0},{"value":0},{"value":2},{"value":2},{"value":0},{"value":0},{"value":0},{"value":305},{"value":8282125520},{"value":0},{"value":5029349288},{"value":5029349352},{"value":481295692000},{"value":0},{"value":0},{"value":0},{"value":256},{"value":1},{"value":0},{"value":0}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6546317776},"cpsr":{"value":1610612736},"fp":{"value":481295691488},"sp":{"value":481295691344},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546048268},"far":{"value":0}}},{"id":9347900,"frames":[{"imageOffset":17676,"symbol":"__psynch_cvwait","symbolLocation":8,"imageIndex":69},{"imageOffset":29136,"symbol":"_pthread_cond_wait","symbolLocation":980,"imageIndex":70},{"imageOffset":26556,"symbol":"std::__1::condition_variable::wait(std::__1::unique_lock<std::__1::mutex>&)","symbolLocation":32,"imageIndex":79},{"imageOffset":260904,"imageIndex":65},{"imageOffset":260560,"imageIndex":65},{"imageOffset":27904,"symbol":"_pthread_start","symbolLocation":136,"imageIndex":70},{"imageOffset":7404,"symbol":"thread_start","symbolLocation":8,"imageIndex":70}],"threadState":{"x":[{"value":260},{"value":0},{"value":0},{"value":0},{"value":0},{"value":160},{"value":0},{"value":0},{"value":481304866408},{"value":0},{"value":0},{"value":2},{"value":2},{"value":0},{"value":0},{"value":0},{"value":305},{"value":8282125520},{"value":0},{"value":5031701104},{"value":5031701168},{"value":481304867040},{"value":0},{"value":0},{"value":0},{"value":256},{"value":1},{"value":0},{"value":0}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6546317776},"cpsr":{"value":1610612736},"fp":{"value":481304866528},"sp":{"value":481304866384},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546048268},"far":{"value":0}}},{"id":9347902,"name":"JUCE v8.0.12: Timer","threadState":{"x":[{"value":260},{"value":0},{"value":186112},{"value":0},{"value":0},{"value":160},{"value":0},{"value":299998792},{"value":481311092248},{"value":0},{"value":1536},{"value":6597069768194},{"value":6597069768194},{"value":1536},{"value":0},{"value":6597069768192},{"value":305},{"value":8282125520},{"value":0},{"value":6049399664},{"value":6049399728},{"value":481311092960},{"value":299998792},{"value":0},{"value":186112},{"value":187136},{"value":186881},{"value":687382206},{"value":4294966381}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6546317776},"cpsr":{"value":1610612736},"fp":{"value":481311092368},"sp":{"value":481311092224},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546048268},"far":{"value":0}},"frames":[{"imageOffset":17676,"symbol":"__psynch_cvwait","symbolLocation":8,"imageIndex":69},{"imageOffset":29136,"symbol":"_pthread_cond_wait","symbolLocation":980,"imageIndex":70},{"imageOffset":26708,"symbol":"std::__1::condition_variable::__do_timed_wait(std::__1::unique_lock<std::__1::mutex>&, std::__1::chrono::time_point<std::__1::chrono::system_clock, std::__1::chrono::duration<long long, std::__1::ratio<1l, 1000000000l>>>)","symbolLocation":104,"imageIndex":79},{"imageOffset":2287780,"symbol":"juce::WaitableEvent::wait(double) const","symbolLocation":576,"imageIndex":66},{"imageOffset":2243404,"symbol":"juce::Timer::TimerThread::run()","symbolLocation":572,"imageIndex":66},{"imageOffset":2452572,"symbol":"juce::Thread::createNativeThread(juce::Thread::Priority)::$_0::__invoke(void*)","symbolLocation":684,"imageIndex":66},{"imageOffset":27904,"symbol":"_pthread_start","symbolLocation":136,"imageIndex":70},{"imageOffset":7404,"symbol":"thread_start","symbolLocation":8,"imageIndex":70}]},{"id":9347903,"frames":[{"imageOffset":17676,"symbol":"__psynch_cvwait","symbolLocation":8,"imageIndex":69},{"imageOffset":29136,"symbol":"_pthread_cond_wait","symbolLocation":980,"imageIndex":70},{"imageOffset":26556,"symbol":"std::__1::condition_variable::wait(std::__1::unique_lock<std::__1::mutex>&)","symbolLocation":32,"imageIndex":79},{"imageOffset":260904,"imageIndex":65},{"imageOffset":260560,"imageIndex":65},{"imageOffset":27904,"symbol":"_pthread_start","symbolLocation":136,"imageIndex":70},{"imageOffset":7404,"symbol":"thread_start","symbolLocation":8,"imageIndex":70}],"threadState":{"x":[{"value":260},{"value":0},{"value":0},{"value":0},{"value":0},{"value":160},{"value":0},{"value":0},{"value":481420095080},{"value":0},{"value":0},{"value":2},{"value":2},{"value":0},{"value":0},{"value":0},{"value":305},{"value":8282125520},{"value":0},{"value":5237253544},{"value":5237253608},{"value":481420095712},{"value":0},{"value":0},{"value":0},{"value":256},{"value":1},{"value":0},{"value":0}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6546317776},"cpsr":{"value":1610612736},"fp":{"value":481420095200},"sp":{"value":481420095056},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546048268},"far":{"value":0}}},{"id":9347904,"frames":[{"imageOffset":17676,"symbol":"__psynch_cvwait","symbolLocation":8,"imageIndex":69},{"imageOffset":29136,"symbol":"_pthread_cond_wait","symbolLocation":980,"imageIndex":70},{"imageOffset":26556,"symbol":"std::__1::condition_variable::wait(std::__1::unique_lock<std::__1::mutex>&)","symbolLocation":32,"imageIndex":79},{"imageOffset":260904,"imageIndex":65},{"imageOffset":260560,"imageIndex":65},{"imageOffset":27904,"symbol":"_pthread_start","symbolLocation":136,"imageIndex":70},{"imageOffset":7404,"symbol":"thread_start","symbolLocation":8,"imageIndex":70}],"threadState":{"x":[{"value":260},{"value":0},{"value":0},{"value":0},{"value":0},{"value":160},{"value":0},{"value":0},{"value":481420668520},{"value":0},{"value":0},{"value":2},{"value":2},{"value":0},{"value":0},{"value":0},{"value":305},{"value":8282125520},{"value":0},{"value":481700847728},{"value":481700847792},{"value":481420669152},{"value":0},{"value":0},{"value":0},{"value":256},{"value":1},{"value":0},{"value":0}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6546317776},"cpsr":{"value":1610612736},"fp":{"value":481420668640},"sp":{"value":481420668496},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546048268},"far":{"value":0}}},{"id":9348371,"name":"bg-worker","threadState":{"x":[{"value":14},{"value":8589934595,"objc-selector":"Փ\u000eu"},{"value":171798697235},{"value":547913272918531},{"value":14680198217728},{"value":547913272918016},{"value":48},{"value":0},{"value":0},{"value":1},{"value":0},{"value":8192},{"value":105553127185584},{"value":2095104},{"value":2043},{"value":3714072746},{"value":18446744073709551580},{"value":8282129856},{"value":0},{"value":105553161913664},{"value":105553161913600},{"value":18446744073709551615},{"value":5324342976},{"value":6048374016},{"value":6},{"value":482464246768},{"value":482464246752},{"value":18446744073709551615},{"value":0}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6544483484},"cpsr":{"value":1610612736},"fp":{"value":482464246528},"sp":{"value":482464246512},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546033564},"far":{"value":0}},"frames":[{"imageOffset":2972,"symbol":"semaphore_wait_trap","symbolLocation":8,"imageIndex":69},{"imageOffset":13468,"symbol":"_dispatch_sema4_wait","symbolLocation":28,"imageIndex":80},{"imageOffset":15144,"symbol":"_dispatch_semaphore_wait_slow","symbolLocation":132,"imageIndex":80},{"imageOffset":1534464,"imageIndex":67},{"imageOffset":405852,"imageIndex":67},{"imageOffset":412644,"imageIndex":67},{"imageOffset":244604,"imageIndex":67},{"imageOffset":360844,"imageIndex":67},{"imageOffset":421772,"imageIndex":67},{"imageOffset":120868,"imageIndex":67},{"imageOffset":1552776,"imageIndex":67},{"imageOffset":27904,"symbol":"_pthread_start","symbolLocation":136,"imageIndex":70},{"imageOffset":7404,"symbol":"thread_start","symbolLocation":8,"imageIndex":70}]},{"id":9349991,"frames":[],"threadState":{"x":[{"value":6123843584},{"value":85235},{"value":6123307008},{"value":0},{"value":409604},{"value":18446744073709551615},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0}],"flavor":"ARM_THREAD_STATE64","lr":{"value":0},"cpsr":{"value":0},"fp":{"value":0},"sp":{"value":6123843584},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546296024},"far":{"value":0}}},{"id":9350132,"name":"com.apple.audio.IOThread.client","threadState":{"x":[{"value":14},{"value":162311},{"value":0},{"value":0},{"value":0},{"value":512},{"value":512},{"value":0},{"value":1},{"value":3298534884098},{"value":3298534884096},{"value":3298534884098},{"value":481189330952},{"value":2},{"value":48},{"value":6},{"value":18446744073709551579},{"value":8282129864},{"value":0},{"value":5360834360},{"value":5360834352},{"value":3},{"value":512},{"value":105553136983048},{"value":105553176334640},{"value":0},{"value":5360834480},{"value":1},{"value":6155660864}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6763741120},"cpsr":{"value":1610612736},"fp":{"value":6155660560},"sp":{"value":6155660544},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546033576},"far":{"value":0}},"frames":[{"imageOffset":2984,"symbol":"semaphore_wait_signal_trap","symbolLocation":8,"imageIndex":69},{"imageOffset":131008,"symbol":"caulk::mach::semaphore::wait_signal_or_error(caulk::mach::semaphore&)","symbolLocation":36,"imageIndex":81},{"imageOffset":6357720,"symbol":"HALC_ProxyIOContext::IOWorkLoop()","symbolLocation":4932,"imageIndex":82},{"imageOffset":1985388,"symbol":"invocation function for block in HALC_ProxyIOContext::HALC_ProxyIOContext(unsigned int, unsigned int)","symbolLocation":172,"imageIndex":82},{"imageOffset":3873908,"symbol":"HALC_IOThread::Entry(void*)","symbolLocation":92,"imageIndex":82},{"imageOffset":27904,"symbol":"_pthread_start","symbolLocation":136,"imageIndex":70},{"imageOffset":7404,"symbol":"thread_start","symbolLocation":8,"imageIndex":70}]},{"id":9350222,"name":"*\/mediaafx","threadState":{"x":[{"value":260},{"value":0},{"value":18565376},{"value":0},{"value":0},{"value":160},{"value":0},{"value":5000000},{"value":18565377},{"value":0},{"value":12288},{"value":52776558145538},{"value":52776558145538},{"value":12288},{"value":0},{"value":52776558145536},{"value":305},{"value":8282125520},{"value":0},{"value":105553157158152},{"value":105553157158216},{"value":1},{"value":5000000},{"value":0},{"value":18565376},{"value":18565632},{"value":18565377},{"value":4354604204,"symbolLocation":0,"symbol":"g_config_workerthread_prio"},{"value":4294967295}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6546317820},"cpsr":{"value":2684354560},"fp":{"value":481191521600},"sp":{"value":481191521456},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546048268},"far":{"value":0}},"frames":[{"imageOffset":17676,"symbol":"__psynch_cvwait","symbolLocation":8,"imageIndex":69},{"imageOffset":29180,"symbol":"_pthread_cond_wait","symbolLocation":1024,"imageIndex":70},{"imageOffset":5441872,"symbol":"WaitForSingleObject(void*, unsigned int)","symbolLocation":668,"imageIndex":0},{"imageOffset":1296388,"symbol":"WorkerThreadInst::threadProc(void*)","symbolLocation":2284,"imageIndex":0},{"imageOffset":5442360,"symbol":"__threadproc(void*)","symbolLocation":32,"imageIndex":0},{"imageOffset":27904,"symbol":"_pthread_start","symbolLocation":136,"imageIndex":70},{"imageOffset":7404,"symbol":"thread_start","symbolLocation":8,"imageIndex":70}]},{"id":9350223,"name":"*\/mediaafx","threadState":{"x":[{"value":260},{"value":0},{"value":18675968},{"value":0},{"value":0},{"value":160},{"value":0},{"value":5000000},{"value":18675969},{"value":0},{"value":7936},{"value":34084860468994},{"value":34084860468994},{"value":7936},{"value":0},{"value":34084860468992},{"value":305},{"value":8282125520},{"value":0},{"value":105553157158536},{"value":105553157158600},{"value":1},{"value":5000000},{"value":0},{"value":18675968},{"value":18676224},{"value":18675969},{"value":4354604204,"symbolLocation":0,"symbol":"g_config_workerthread_prio"},{"value":4294967295}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6546317820},"cpsr":{"value":2684354560},"fp":{"value":481192095040},"sp":{"value":481192094896},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546048268},"far":{"value":0}},"frames":[{"imageOffset":17676,"symbol":"__psynch_cvwait","symbolLocation":8,"imageIndex":69},{"imageOffset":29180,"symbol":"_pthread_cond_wait","symbolLocation":1024,"imageIndex":70},{"imageOffset":5441872,"symbol":"WaitForSingleObject(void*, unsigned int)","symbolLocation":668,"imageIndex":0},{"imageOffset":1296388,"symbol":"WorkerThreadInst::threadProc(void*)","symbolLocation":2284,"imageIndex":0},{"imageOffset":5442360,"symbol":"__threadproc(void*)","symbolLocation":32,"imageIndex":0},{"imageOffset":27904,"symbol":"_pthread_start","symbolLocation":136,"imageIndex":70},{"imageOffset":7404,"symbol":"thread_start","symbolLocation":8,"imageIndex":70}]},{"id":9350224,"name":"*\/mediaafx","threadState":{"x":[{"value":316},{"value":0},{"value":18627072},{"value":0},{"value":0},{"value":160},{"value":0},{"value":5000000},{"value":18627073},{"value":0},{"value":6912},{"value":29686813956866},{"value":29686813956866},{"value":6912},{"value":0},{"value":29686813956864},{"value":305},{"value":8282125520},{"value":0},{"value":105553157158664},{"value":105553157158728},{"value":1},{"value":5000000},{"value":0},{"value":18627072},{"value":18627328},{"value":18627073},{"value":4354604204,"symbolLocation":0,"symbol":"g_config_workerthread_prio"},{"value":4294967295}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6546317820},"cpsr":{"value":2684354560},"fp":{"value":481192668480},"sp":{"value":481192668336},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546048268},"far":{"value":0}},"frames":[{"imageOffset":17676,"symbol":"__psynch_cvwait","symbolLocation":8,"imageIndex":69},{"imageOffset":29180,"symbol":"_pthread_cond_wait","symbolLocation":1024,"imageIndex":70},{"imageOffset":5441872,"symbol":"WaitForSingleObject(void*, unsigned int)","symbolLocation":668,"imageIndex":0},{"imageOffset":1296388,"symbol":"WorkerThreadInst::threadProc(void*)","symbolLocation":2284,"imageIndex":0},{"imageOffset":5442360,"symbol":"__threadproc(void*)","symbolLocation":32,"imageIndex":0},{"imageOffset":27904,"symbol":"_pthread_start","symbolLocation":136,"imageIndex":70},{"imageOffset":7404,"symbol":"thread_start","symbolLocation":8,"imageIndex":70}]},{"id":9350225,"name":"*\/mediaafx","threadState":{"x":[{"value":260},{"value":0},{"value":18587648},{"value":0},{"value":0},{"value":160},{"value":0},{"value":5000000},{"value":18587649},{"value":0},{"value":6144},{"value":26388279072770},{"value":26388279072770},{"value":6144},{"value":0},{"value":26388279072768},{"value":305},{"value":8282125520},{"value":0},{"value":105553157158792},{"value":105553157158856},{"value":1},{"value":5000000},{"value":0},{"value":18587648},{"value":18587904},{"value":18587649},{"value":4354604204,"symbolLocation":0,"symbol":"g_config_workerthread_prio"},{"value":4294967295}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6546317820},"cpsr":{"value":2684354560},"fp":{"value":481193241920},"sp":{"value":481193241776},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546048268},"far":{"value":0}},"frames":[{"imageOffset":17676,"symbol":"__psynch_cvwait","symbolLocation":8,"imageIndex":69},{"imageOffset":29180,"symbol":"_pthread_cond_wait","symbolLocation":1024,"imageIndex":70},{"imageOffset":5441872,"symbol":"WaitForSingleObject(void*, unsigned int)","symbolLocation":668,"imageIndex":0},{"imageOffset":1296388,"symbol":"WorkerThreadInst::threadProc(void*)","symbolLocation":2284,"imageIndex":0},{"imageOffset":5442360,"symbol":"__threadproc(void*)","symbolLocation":32,"imageIndex":0},{"imageOffset":27904,"symbol":"_pthread_start","symbolLocation":136,"imageIndex":70},{"imageOffset":7404,"symbol":"thread_start","symbolLocation":8,"imageIndex":70}]},{"id":9350226,"name":"*\/mediaafx","threadState":{"x":[{"value":260},{"value":0},{"value":18634496},{"value":0},{"value":0},{"value":160},{"value":0},{"value":5000000},{"value":18634497},{"value":0},{"value":7168},{"value":30786325584898},{"value":30786325584898},{"value":7168},{"value":0},{"value":30786325584896},{"value":305},{"value":8282125520},{"value":0},{"value":105553157158920},{"value":105553157158984},{"value":1},{"value":5000000},{"value":0},{"value":18634496},{"value":18634752},{"value":18634497},{"value":4354604204,"symbolLocation":0,"symbol":"g_config_workerthread_prio"},{"value":4294967295}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6546317820},"cpsr":{"value":2684354560},"fp":{"value":481193815360},"sp":{"value":481193815216},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546048268},"far":{"value":0}},"frames":[{"imageOffset":17676,"symbol":"__psynch_cvwait","symbolLocation":8,"imageIndex":69},{"imageOffset":29180,"symbol":"_pthread_cond_wait","symbolLocation":1024,"imageIndex":70},{"imageOffset":5441872,"symbol":"WaitForSingleObject(void*, unsigned int)","symbolLocation":668,"imageIndex":0},{"imageOffset":1296388,"symbol":"WorkerThreadInst::threadProc(void*)","symbolLocation":2284,"imageIndex":0},{"imageOffset":5442360,"symbol":"__threadproc(void*)","symbolLocation":32,"imageIndex":0},{"imageOffset":27904,"symbol":"_pthread_start","symbolLocation":136,"imageIndex":70},{"imageOffset":7404,"symbol":"thread_start","symbolLocation":8,"imageIndex":70}]},{"id":9350227,"name":"*\/mediaafx","threadState":{"x":[{"value":260},{"value":0},{"value":18652928},{"value":0},{"value":0},{"value":160},{"value":0},{"value":5000000},{"value":18652929},{"value":0},{"value":6144},{"value":26388279072770},{"value":26388279072770},{"value":6144},{"value":0},{"value":26388279072768},{"value":305},{"value":8282125520},{"value":0},{"value":105553157159048},{"value":105553157159112},{"value":1},{"value":5000000},{"value":0},{"value":18652928},{"value":18653184},{"value":18652929},{"value":4354604204,"symbolLocation":0,"symbol":"g_config_workerthread_prio"},{"value":4294967295}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6546317820},"cpsr":{"value":2684354560},"fp":{"value":481194388800},"sp":{"value":481194388656},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546048268},"far":{"value":0}},"frames":[{"imageOffset":17676,"symbol":"__psynch_cvwait","symbolLocation":8,"imageIndex":69},{"imageOffset":29180,"symbol":"_pthread_cond_wait","symbolLocation":1024,"imageIndex":70},{"imageOffset":5441872,"symbol":"WaitForSingleObject(void*, unsigned int)","symbolLocation":668,"imageIndex":0},{"imageOffset":1296388,"symbol":"WorkerThreadInst::threadProc(void*)","symbolLocation":2284,"imageIndex":0},{"imageOffset":5442360,"symbol":"__threadproc(void*)","symbolLocation":32,"imageIndex":0},{"imageOffset":27904,"symbol":"_pthread_start","symbolLocation":136,"imageIndex":70},{"imageOffset":7404,"symbol":"thread_start","symbolLocation":8,"imageIndex":70}]},{"id":9350228,"name":"*\/mediaafx","threadState":{"x":[{"value":316},{"value":0},{"value":18647040},{"value":0},{"value":0},{"value":160},{"value":0},{"value":5000000},{"value":18647041},{"value":0},{"value":6656},{"value":28587302328834},{"value":28587302328834},{"value":6656},{"value":0},{"value":28587302328832},{"value":305},{"value":8282125520},{"value":0},{"value":105553157159176},{"value":105553157159240},{"value":1},{"value":5000000},{"value":0},{"value":18647040},{"value":18647296},{"value":18647041},{"value":4354604204,"symbolLocation":0,"symbol":"g_config_workerthread_prio"},{"value":4294967295}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6546317820},"cpsr":{"value":2684354560},"fp":{"value":481194962240},"sp":{"value":481194962096},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546048268},"far":{"value":0}},"frames":[{"imageOffset":17676,"symbol":"__psynch_cvwait","symbolLocation":8,"imageIndex":69},{"imageOffset":29180,"symbol":"_pthread_cond_wait","symbolLocation":1024,"imageIndex":70},{"imageOffset":5441872,"symbol":"WaitForSingleObject(void*, unsigned int)","symbolLocation":668,"imageIndex":0},{"imageOffset":1296388,"symbol":"WorkerThreadInst::threadProc(void*)","symbolLocation":2284,"imageIndex":0},{"imageOffset":5442360,"symbol":"__threadproc(void*)","symbolLocation":32,"imageIndex":0},{"imageOffset":27904,"symbol":"_pthread_start","symbolLocation":136,"imageIndex":70},{"imageOffset":7404,"symbol":"thread_start","symbolLocation":8,"imageIndex":70}]},{"id":9350229,"name":"*\/mediaafx","threadState":{"x":[{"value":316},{"value":0},{"value":18668032},{"value":0},{"value":0},{"value":160},{"value":0},{"value":5000000},{"value":18668033},{"value":0},{"value":5376},{"value":23089744188674},{"value":23089744188674},{"value":5376},{"value":0},{"value":23089744188672},{"value":305},{"value":8282125520},{"value":0},{"value":105553157159304},{"value":105553157159368},{"value":1},{"value":5000000},{"value":0},{"value":18668032},{"value":18668288},{"value":18668033},{"value":4354604204,"symbolLocation":0,"symbol":"g_config_workerthread_prio"},{"value":4294967295}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6546317820},"cpsr":{"value":2684354560},"fp":{"value":481294544192},"sp":{"value":481294544048},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546048268},"far":{"value":0}},"frames":[{"imageOffset":17676,"symbol":"__psynch_cvwait","symbolLocation":8,"imageIndex":69},{"imageOffset":29180,"symbol":"_pthread_cond_wait","symbolLocation":1024,"imageIndex":70},{"imageOffset":5441872,"symbol":"WaitForSingleObject(void*, unsigned int)","symbolLocation":668,"imageIndex":0},{"imageOffset":1296388,"symbol":"WorkerThreadInst::threadProc(void*)","symbolLocation":2284,"imageIndex":0},{"imageOffset":5442360,"symbol":"__threadproc(void*)","symbolLocation":32,"imageIndex":0},{"imageOffset":27904,"symbol":"_pthread_start","symbolLocation":136,"imageIndex":70},{"imageOffset":7404,"symbol":"thread_start","symbolLocation":8,"imageIndex":70}]},{"id":9350230,"name":"*\/mediaafx","threadState":{"x":[{"value":260},{"value":0},{"value":18684672},{"value":0},{"value":0},{"value":160},{"value":0},{"value":5000000},{"value":18684673},{"value":0},{"value":7168},{"value":30786325584898},{"value":30786325584898},{"value":7168},{"value":0},{"value":30786325584896},{"value":305},{"value":8282125520},{"value":0},{"value":105553157159432},{"value":105553157159496},{"value":1},{"value":5000000},{"value":0},{"value":18684672},{"value":18684928},{"value":18684673},{"value":4354604204,"symbolLocation":0,"symbol":"g_config_workerthread_prio"},{"value":4294967295}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6546317820},"cpsr":{"value":2684354560},"fp":{"value":481295117632},"sp":{"value":481295117488},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546048268},"far":{"value":0}},"frames":[{"imageOffset":17676,"symbol":"__psynch_cvwait","symbolLocation":8,"imageIndex":69},{"imageOffset":29180,"symbol":"_pthread_cond_wait","symbolLocation":1024,"imageIndex":70},{"imageOffset":5441872,"symbol":"WaitForSingleObject(void*, unsigned int)","symbolLocation":668,"imageIndex":0},{"imageOffset":1296388,"symbol":"WorkerThreadInst::threadProc(void*)","symbolLocation":2284,"imageIndex":0},{"imageOffset":5442360,"symbol":"__threadproc(void*)","symbolLocation":32,"imageIndex":0},{"imageOffset":27904,"symbol":"_pthread_start","symbolLocation":136,"imageIndex":70},{"imageOffset":7404,"symbol":"thread_start","symbolLocation":8,"imageIndex":70}]},{"id":9350231,"name":"*\/mediaafx","threadState":{"x":[{"value":316},{"value":0},{"value":18688512},{"value":0},{"value":0},{"value":160},{"value":0},{"value":5000000},{"value":18688513},{"value":0},{"value":7936},{"value":34084860468994},{"value":34084860468994},{"value":7936},{"value":0},{"value":34084860468992},{"value":305},{"value":8282125520},{"value":0},{"value":105553157159560},{"value":105553157159624},{"value":1},{"value":5000000},{"value":0},{"value":18688512},{"value":18688768},{"value":18688513},{"value":4354604204,"symbolLocation":0,"symbol":"g_config_workerthread_prio"},{"value":4294967295}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6546317820},"cpsr":{"value":2684354560},"fp":{"value":481415408960},"sp":{"value":481415408816},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546048268},"far":{"value":0}},"frames":[{"imageOffset":17676,"symbol":"__psynch_cvwait","symbolLocation":8,"imageIndex":69},{"imageOffset":29180,"symbol":"_pthread_cond_wait","symbolLocation":1024,"imageIndex":70},{"imageOffset":5441872,"symbol":"WaitForSingleObject(void*, unsigned int)","symbolLocation":668,"imageIndex":0},{"imageOffset":1296388,"symbol":"WorkerThreadInst::threadProc(void*)","symbolLocation":2284,"imageIndex":0},{"imageOffset":5442360,"symbol":"__threadproc(void*)","symbolLocation":32,"imageIndex":0},{"imageOffset":27904,"symbol":"_pthread_start","symbolLocation":136,"imageIndex":70},{"imageOffset":7404,"symbol":"thread_start","symbolLocation":8,"imageIndex":70}]},{"id":9350232,"name":"*\/livefx","threadState":{"x":[{"value":260},{"value":0},{"value":893440},{"value":0},{"value":0},{"value":160},{"value":0},{"value":20000000},{"value":4772353},{"value":0},{"value":0},{"value":2},{"value":2},{"value":0},{"value":0},{"value":0},{"value":305},{"value":8282125520},{"value":0},{"value":105553157159688},{"value":105553157159752},{"value":1},{"value":20000000},{"value":0},{"value":893440},{"value":4772608},{"value":4772353},{"value":0},{"value":0}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6546317820},"cpsr":{"value":2684354560},"fp":{"value":481416703296},"sp":{"value":481416703152},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546048268},"far":{"value":0}},"frames":[{"imageOffset":17676,"symbol":"__psynch_cvwait","symbolLocation":8,"imageIndex":69},{"imageOffset":29180,"symbol":"_pthread_cond_wait","symbolLocation":1024,"imageIndex":70},{"imageOffset":5441872,"symbol":"WaitForSingleObject(void*, unsigned int)","symbolLocation":668,"imageIndex":0},{"imageOffset":1297212,"symbol":"WorkerThreadInst::threadProc(void*)","symbolLocation":3108,"imageIndex":0},{"imageOffset":5442360,"symbol":"__threadproc(void*)","symbolLocation":32,"imageIndex":0},{"imageOffset":27904,"symbol":"_pthread_start","symbolLocation":136,"imageIndex":70},{"imageOffset":7404,"symbol":"thread_start","symbolLocation":8,"imageIndex":70}]},{"id":9350233,"name":"*\/livefx","threadState":{"x":[{"value":260},{"value":0},{"value":893440},{"value":0},{"value":0},{"value":160},{"value":0},{"value":20000000},{"value":4772353},{"value":0},{"value":0},{"value":2},{"value":2},{"value":0},{"value":0},{"value":0},{"value":305},{"value":8282125520},{"value":0},{"value":105553157159816},{"value":105553157159880},{"value":1},{"value":20000000},{"value":0},{"value":893440},{"value":4772608},{"value":4772353},{"value":0},{"value":0}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6546317820},"cpsr":{"value":2684354560},"fp":{"value":481417276736},"sp":{"value":481417276592},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546048268},"far":{"value":0}},"frames":[{"imageOffset":17676,"symbol":"__psynch_cvwait","symbolLocation":8,"imageIndex":69},{"imageOffset":29180,"symbol":"_pthread_cond_wait","symbolLocation":1024,"imageIndex":70},{"imageOffset":5441872,"symbol":"WaitForSingleObject(void*, unsigned int)","symbolLocation":668,"imageIndex":0},{"imageOffset":1297212,"symbol":"WorkerThreadInst::threadProc(void*)","symbolLocation":3108,"imageIndex":0},{"imageOffset":5442360,"symbol":"__threadproc(void*)","symbolLocation":32,"imageIndex":0},{"imageOffset":27904,"symbol":"_pthread_start","symbolLocation":136,"imageIndex":70},{"imageOffset":7404,"symbol":"thread_start","symbolLocation":8,"imageIndex":70}]},{"id":9350234,"name":"*\/livefx","threadState":{"x":[{"value":260},{"value":0},{"value":893440},{"value":0},{"value":0},{"value":160},{"value":0},{"value":20000000},{"value":4772353},{"value":0},{"value":0},{"value":2},{"value":2},{"value":0},{"value":0},{"value":0},{"value":305},{"value":8282125520},{"value":0},{"value":105553157159944},{"value":105553157160008},{"value":1},{"value":20000000},{"value":0},{"value":893440},{"value":4772608},{"value":4772353},{"value":0},{"value":0}],"flavor":"ARM_THREAD_STATE64","lr":{"value":6546317820},"cpsr":{"value":2684354560},"fp":{"value":481417850176},"sp":{"value":481417850032},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546048268},"far":{"value":0}},"frames":[{"imageOffset":17676,"symbol":"__psynch_cvwait","symbolLocation":8,"imageIndex":69},{"imageOffset":29180,"symbol":"_pthread_cond_wait","symbolLocation":1024,"imageIndex":70},{"imageOffset":5441872,"symbol":"WaitForSingleObject(void*, unsigned int)","symbolLocation":668,"imageIndex":0},{"imageOffset":1297212,"symbol":"WorkerThreadInst::threadProc(void*)","symbolLocation":3108,"imageIndex":0},{"imageOffset":5442360,"symbol":"__threadproc(void*)","symbolLocation":32,"imageIndex":0},{"imageOffset":27904,"symbol":"_pthread_start","symbolLocation":136,"imageIndex":70},{"imageOffset":7404,"symbol":"thread_start","symbolLocation":8,"imageIndex":70}]},{"id":9351606,"frames":[],"threadState":{"x":[{"value":6124417024},{"value":182459},{"value":6123880448},{"value":0},{"value":409604},{"value":18446744073709551615},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0}],"flavor":"ARM_THREAD_STATE64","lr":{"value":0},"cpsr":{"value":0},"fp":{"value":0},"sp":{"value":6124417024},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546296024},"far":{"value":0}}},{"id":9351607,"frames":[],"threadState":{"x":[{"value":6127284224},{"value":255003},{"value":6126747648},{"value":0},{"value":409604},{"value":18446744073709551615},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0}],"flavor":"ARM_THREAD_STATE64","lr":{"value":0},"cpsr":{"value":0},"fp":{"value":0},"sp":{"value":6127284224},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546296024},"far":{"value":0}}},{"id":9352822,"frames":[],"threadState":{"x":[{"value":6126710784},{"value":183283},{"value":6126174208},{"value":0},{"value":409604},{"value":18446744073709551615},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0}],"flavor":"ARM_THREAD_STATE64","lr":{"value":0},"cpsr":{"value":0},"fp":{"value":0},"sp":{"value":6126710784},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546296024},"far":{"value":0}}},{"id":9352823,"frames":[],"threadState":{"x":[{"value":6128431104},{"value":188931},{"value":6127894528},{"value":0},{"value":409604},{"value":18446744073709551615},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0}],"flavor":"ARM_THREAD_STATE64","lr":{"value":0},"cpsr":{"value":0},"fp":{"value":0},"sp":{"value":6128431104},"esr":{"value":1442840704,"description":"(Syscall)"},"pc":{"value":6546296024},"far":{"value":0}}},{"id":9353455,"frames":[],"threadState":{"x":[{"value":481291137024},{"value":0},{"value":481290600448},{"value":0},{"value":278532},{"value":18446744073709551615},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0},{"value":0}],"flavor":"ARM_THREAD_STATE64","lr":{"value":0},"cpsr":{"value":0},"fp":{"value":0},"sp":{"value":481291137024},"esr":{"value":0},"pc":{"value":6546296024},"far":{"value":0}}}],
  "usedImages" : [
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 4343595008,
    "CFBundleShortVersionString" : "7.76.0_6cb0d69u",
    "CFBundleIdentifier" : "com.cockos.reaper",
    "size" : 10567680,
    "uuid" : "63af0b43-88d0-345e-80d7-5bdce53efce9",
    "path" : "\/Applications\/REAPER.app\/Contents\/MacOS\/REAPER",
    "name" : "REAPER",
    "CFBundleVersion" : "7.76.0_6cb0d69u"
  },
  {
    "source" : "P",
    "arch" : "arm64e",
    "base" : 4360388608,
    "size" : 49152,
    "uuid" : "9ef9e491-f3d3-322c-918a-559d3b858d9a",
    "path" : "\/usr\/lib\/libobjc-trampolines.dylib",
    "name" : "libobjc-trampolines.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64e",
    "base" : 4629348352,
    "size" : 114688,
    "uuid" : "b7d8c7ba-8c30-3362-b3c7-d0998077dc1d",
    "path" : "\/Volumes\/VOLUME\/*\/libMetalMetricsInterpose.dylib",
    "name" : "libMetalMetricsInterpose.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 4996087808,
    "size" : 196608,
    "uuid" : "fdeee20a-415a-3c11-b3e0-2f7a3d2ffb81",
    "path" : "\/Applications\/REAPER.app\/Contents\/Plugins\/elastique3.dylib",
    "name" : "elastique3.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 4995416064,
    "size" : 32768,
    "uuid" : "b467a3bf-d8d5-30c0-a58e-945a0ce2c239",
    "path" : "\/Applications\/REAPER.app\/Contents\/Plugins\/soundtouch.dylib",
    "name" : "soundtouch.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 4747493376,
    "size" : 114688,
    "uuid" : "e8b80eeb-df32-3f3f-99d5-8c869d457618",
    "path" : "\/Applications\/REAPER.app\/Contents\/Plugins\/rubberband.dylib",
    "name" : "rubberband.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 4995514368,
    "size" : 360448,
    "uuid" : "1a896670-e67d-3d49-88dd-cabca854a415",
    "path" : "\/Applications\/REAPER.app\/Contents\/Plugins\/jsfx.dylib",
    "name" : "jsfx.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 4745986048,
    "size" : 147456,
    "uuid" : "ba5355e0-01c0-31c8-827e-bc457175245a",
    "path" : "\/Applications\/REAPER.app\/Contents\/Plugins\/reaper_wave.dylib",
    "name" : "reaper_wave.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5017026560,
    "size" : 163840,
    "uuid" : "5c3d5154-798c-35da-bf71-d3b736a81df4",
    "path" : "\/Applications\/REAPER.app\/Contents\/Plugins\/reaper_mp3dec.dylib",
    "name" : "reaper_mp3dec.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5019254784,
    "size" : 294912,
    "uuid" : "22134e04-d839-3b7c-a8ee-3aa135952a89",
    "path" : "\/Applications\/REAPER.app\/Contents\/Plugins\/libmp3lame.dylib",
    "name" : "libmp3lame.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5023023104,
    "size" : 1228800,
    "uuid" : "981f8b48-8c07-33ad-8fc4-0043cc789863",
    "path" : "\/Applications\/REAPER.app\/Contents\/Plugins\/reaper_midi.dylib",
    "name" : "reaper_midi.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5019828224,
    "size" : 196608,
    "uuid" : "3c5acd7b-deff-3364-a7ae-635d353222a6",
    "path" : "\/Applications\/REAPER.app\/Contents\/Plugins\/reaper_wavpack.dylib",
    "name" : "reaper_wavpack.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5018484736,
    "size" : 458752,
    "uuid" : "13ce821c-8164-303c-9376-14ada1d65c12",
    "path" : "\/Applications\/REAPER.app\/Contents\/Plugins\/reaper_opus.dylib",
    "name" : "reaper_opus.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5020762112,
    "size" : 196608,
    "uuid" : "54e5c255-fa88-38e9-95ff-74c414b70927",
    "path" : "\/Applications\/REAPER.app\/Contents\/Plugins\/reaper_flac.dylib",
    "name" : "reaper_flac.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5020483584,
    "size" : 65536,
    "uuid" : "34e67550-7d19-3ba8-bcd3-6fd90b60e780",
    "path" : "\/Applications\/REAPER.app\/Contents\/Plugins\/reaper_rex.dylib",
    "name" : "reaper_rex.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 4746493952,
    "size" : 49152,
    "uuid" : "f0c06206-3678-308d-986f-cc50f60ea061",
    "path" : "\/Applications\/REAPER.app\/Contents\/Plugins\/reaper_ddp.dylib",
    "name" : "reaper_ddp.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5022007296,
    "size" : 327680,
    "uuid" : "daed245b-073b-35bd-93a0-5be72df093d8",
    "path" : "\/Applications\/REAPER.app\/Contents\/Plugins\/reaper_explorer.dylib",
    "name" : "reaper_explorer.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5019074560,
    "size" : 49152,
    "uuid" : "056b3fca-d3df-34ad-8a18-97ec0982aa79",
    "path" : "\/Applications\/REAPER.app\/Contents\/Plugins\/reaper_cd.dylib",
    "name" : "reaper_cd.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5035180032,
    "size" : 688128,
    "uuid" : "4328fdf4-6cff-3b59-8602-cdb8a829d878",
    "path" : "\/Applications\/REAPER.app\/Contents\/Plugins\/reaper_ogg.dylib",
    "name" : "reaper_ogg.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5022482432,
    "size" : 262144,
    "uuid" : "beba3477-3808-3dec-ba8a-cddff1b94f65",
    "path" : "\/Applications\/REAPER.app\/Contents\/Plugins\/reaper_csurf.dylib",
    "name" : "reaper_csurf.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5033164800,
    "size" : 1015808,
    "uuid" : "f48aab88-2228-3f9f-946c-58e30e57428f",
    "path" : "\/Applications\/REAPER.app\/Contents\/Plugins\/reaper_video.dylib",
    "name" : "reaper_video.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5020352512,
    "size" : 32768,
    "uuid" : "30492a04-5521-3661-8c24-5cf77c5b71ac",
    "path" : "\/Applications\/REAPER.app\/Contents\/Plugins\/reap_osx_modern.dylib",
    "name" : "reap_osx_modern.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5036179456,
    "size" : 475136,
    "uuid" : "c65071ae-5ed4-3e54-b56c-4b625005edbc",
    "path" : "\/opt\/homebrew\/*\/libavutil.60.26.101.dylib",
    "name" : "libavutil.60.26.101.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5037932544,
    "size" : 589824,
    "uuid" : "704bf8e6-0dec-398e-986f-2eb58f3c6b3c",
    "path" : "\/opt\/homebrew\/*\/libssl.3.dylib",
    "name" : "libssl.3.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5043666944,
    "size" : 3407872,
    "uuid" : "6d7e86a1-ba7d-3b61-bfc7-248b1221632e",
    "path" : "\/opt\/homebrew\/*\/libcrypto.3.dylib",
    "name" : "libcrypto.3.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5038817280,
    "size" : 524288,
    "uuid" : "50bfe305-8558-335e-b305-09019ce57377",
    "path" : "\/opt\/homebrew\/*\/libswscale.9.5.101.dylib",
    "name" : "libswscale.9.5.101.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5020237824,
    "size" : 65536,
    "uuid" : "e59853f8-fd79-3cc8-95d6-95ff184ba81f",
    "path" : "\/opt\/homebrew\/*\/libswresample.6.3.101.dylib",
    "name" : "libswresample.6.3.101.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5058166784,
    "size" : 9109504,
    "uuid" : "49303757-5681-368c-9db2-af5b5218adb6",
    "path" : "\/opt\/homebrew\/*\/libavcodec.62.28.101.dylib",
    "name" : "libavcodec.62.28.101.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5041471488,
    "size" : 1540096,
    "uuid" : "abef3286-0a7b-3e10-8825-81c8a863efa8",
    "path" : "\/opt\/homebrew\/*\/libvpx.12.dylib",
    "name" : "libvpx.12.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5039702016,
    "size" : 622592,
    "uuid" : "b317db1b-7e5e-3ad6-8dfb-5f30139ba348",
    "path" : "\/opt\/homebrew\/*\/libdav1d.7.dylib",
    "name" : "libdav1d.7.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5021384704,
    "size" : 212992,
    "uuid" : "de863ab7-3cf8-38fe-ae37-59a526573730",
    "path" : "\/opt\/homebrew\/*\/libmp3lame.0.dylib",
    "name" : "libmp3lame.0.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5040734208,
    "size" : 311296,
    "uuid" : "1c7445f6-fb0e-33ea-8aa8-18597a9443ac",
    "path" : "\/opt\/homebrew\/*\/libopus.0.dylib",
    "name" : "libopus.0.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5051416576,
    "size" : 2572288,
    "uuid" : "06e58860-87c7-3851-bb90-8d08a10ae2e7",
    "path" : "\/opt\/homebrew\/*\/libSvtAv1Enc.4.1.0.dylib",
    "name" : "libSvtAv1Enc.4.1.0.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5054562304,
    "size" : 1179648,
    "uuid" : "b916484f-5239-335b-b9fe-4e71261da16b",
    "path" : "\/opt\/homebrew\/*\/libx264.165.dylib",
    "name" : "libx264.165.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5082251264,
    "size" : 6160384,
    "uuid" : "710dc56c-1d6c-3b7b-bba6-157396ddc818",
    "path" : "\/opt\/homebrew\/*\/libx265.216.dylib",
    "name" : "libx265.216.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5074763776,
    "size" : 2015232,
    "uuid" : "da948b54-d8e2-3e3d-8efc-097ee2fbc2a7",
    "path" : "\/opt\/homebrew\/*\/libavformat.62.12.101.dylib",
    "name" : "libavformat.62.12.101.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5049450496,
    "size" : 802816,
    "uuid" : "600321d1-62bd-395f-8699-aed018555ae1",
    "path" : "\/Applications\/VLC.app\/Contents\/MacOS\/lib\/libvlccore.9.dylib",
    "name" : "libvlccore.9.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 6130171904,
    "size" : 13467648,
    "uuid" : "3da6d39e-ac45-3e06-acd0-b66c162cc860",
    "path" : "\/Applications\/VLC.app\/Contents\/MacOS\/plugins\/libavcodec_plugin.dylib",
    "name" : "libavcodec_plugin.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5024563200,
    "size" : 98304,
    "uuid" : "60f2f0e6-8acf-3f55-859c-17546d85ac58",
    "path" : "\/Applications\/VLC.app\/Contents\/MacOS\/lib\/libvlc.5.dylib",
    "name" : "libvlc.5.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5021073408,
    "size" : 32768,
    "uuid" : "e5ee2c91-b180-3082-9fec-f624c83e63af",
    "path" : "\/Applications\/VLC.app\/Contents\/MacOS\/plugins\/libi420_rgb_plugin.dylib",
    "name" : "libi420_rgb_plugin.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5020680192,
    "size" : 16384,
    "uuid" : "f3610263-f7e8-31cb-9d9d-8eced5725499",
    "path" : "\/Applications\/VLC.app\/Contents\/MacOS\/plugins\/libi420_10_p010_plugin.dylib",
    "name" : "libi420_10_p010_plugin.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5021171712,
    "size" : 16384,
    "uuid" : "9752c99b-c2a5-3a9f-a9e3-e14bfe735860",
    "path" : "\/Applications\/VLC.app\/Contents\/MacOS\/plugins\/libscaletempo_pitch_plugin.dylib",
    "name" : "libscaletempo_pitch_plugin.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5021253632,
    "size" : 16384,
    "uuid" : "858f1447-7e20-3047-9a05-b313704bd9c1",
    "path" : "\/Applications\/VLC.app\/Contents\/MacOS\/plugins\/libi420_yuy2_plugin.dylib",
    "name" : "libi420_yuy2_plugin.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5021876224,
    "size" : 16384,
    "uuid" : "ca4cf604-6588-39b3-91e7-9b1d5a2677a5",
    "path" : "\/Applications\/VLC.app\/Contents\/MacOS\/plugins\/libi422_yuy2_plugin.dylib",
    "name" : "libi422_yuy2_plugin.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5022892032,
    "size" : 16384,
    "uuid" : "2dae5195-de46-32ef-9929-3f0dee40f299",
    "path" : "\/Applications\/VLC.app\/Contents\/MacOS\/plugins\/libyuy2_i420_plugin.dylib",
    "name" : "libyuy2_i420_plugin.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5034508288,
    "size" : 32768,
    "uuid" : "d107acae-a835-3cc4-bd43-a200af1b8c24",
    "path" : "\/Applications\/VLC.app\/Contents\/MacOS\/plugins\/libi420_nv12_plugin.dylib",
    "name" : "libi420_nv12_plugin.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5034606592,
    "size" : 16384,
    "uuid" : "a0b9902a-a6e1-3cf7-bc76-a9997640a925",
    "path" : "\/Applications\/VLC.app\/Contents\/MacOS\/plugins\/libyuy2_i422_plugin.dylib",
    "name" : "libyuy2_i422_plugin.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5034688512,
    "size" : 32768,
    "uuid" : "3c6552be-64b8-3221-9232-985d8666f08b",
    "path" : "\/Applications\/VLC.app\/Contents\/MacOS\/plugins\/libkeychain_plugin.dylib",
    "name" : "libkeychain_plugin.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5034786816,
    "size" : 16384,
    "uuid" : "4279056d-c9e3-3961-beab-8b886b9d18d3",
    "path" : "\/Applications\/VLC.app\/Contents\/MacOS\/plugins\/libi422_i420_plugin.dylib",
    "name" : "libi422_i420_plugin.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5034868736,
    "size" : 16384,
    "uuid" : "d57407c8-8e20-3672-a170-48b1af424fb8",
    "path" : "\/Applications\/VLC.app\/Contents\/MacOS\/plugins\/libgrey_yuv_plugin.dylib",
    "name" : "libgrey_yuv_plugin.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5034950656,
    "size" : 16384,
    "uuid" : "11fe731b-8256-3bff-9f23-c3e40e4db6b0",
    "path" : "\/Applications\/VLC.app\/Contents\/MacOS\/plugins\/libchain_plugin.dylib",
    "name" : "libchain_plugin.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5035032576,
    "size" : 16384,
    "uuid" : "482bb48a-a07d-3dbc-89a7-0d594e17796d",
    "path" : "\/Applications\/VLC.app\/Contents\/MacOS\/plugins\/libscale_plugin.dylib",
    "name" : "libscale_plugin.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5041111040,
    "size" : 16384,
    "uuid" : "fac3d456-4c92-372a-b223-14d1969c13cc",
    "path" : "\/Applications\/VLC.app\/Contents\/MacOS\/plugins\/libyuvp_plugin.dylib",
    "name" : "libyuvp_plugin.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5056856064,
    "size" : 819200,
    "uuid" : "81f4eb4e-87ff-3445-801c-2ecc40351645",
    "path" : "\/Applications\/VLC.app\/Contents\/MacOS\/plugins\/libdav1d_plugin.dylib",
    "name" : "libdav1d_plugin.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5041192960,
    "size" : 16384,
    "uuid" : "6446a71d-d909-3354-960f-27c7efabc9a6",
    "path" : "\/Applications\/VLC.app\/Contents\/MacOS\/plugins\/libyuv_plugin.dylib",
    "name" : "libyuv_plugin.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5041274880,
    "size" : 16384,
    "uuid" : "23f21c98-24f3-3a7e-a975-ed7fc87097e8",
    "path" : "\/Applications\/VLC.app\/Contents\/MacOS\/plugins\/libscaletempo_plugin.dylib",
    "name" : "libscaletempo_plugin.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5077123072,
    "size" : 688128,
    "uuid" : "6d9def6a-e09f-3d0b-9a39-487a972b513b",
    "path" : "\/Applications\/VLC.app\/Contents\/MacOS\/plugins\/libswscale_plugin.dylib",
    "name" : "libswscale_plugin.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5089820672,
    "size" : 2572288,
    "uuid" : "c7798dc2-75c9-38fe-b9a4-235e2091f679",
    "path" : "\/Users\/USER\/Library\/Application Support\/REAPER\/*\/reaper_imgui-arm64.dylib",
    "name" : "reaper_imgui-arm64.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5079875584,
    "size" : 1179648,
    "uuid" : "1ec74161-eae1-325c-bab0-3eec253a12a7",
    "path" : "\/Users\/USER\/Library\/Application Support\/REAPER\/*\/reaper_js_ReaScriptAPI64ARM.dylib",
    "name" : "reaper_js_ReaScriptAPI64ARM.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5095571456,
    "size" : 1720320,
    "uuid" : "99d2acf4-bfe6-3926-8582-d8e8688cafb4",
    "path" : "\/Users\/USER\/Library\/Application Support\/REAPER\/*\/reaper_reapack-arm64.dylib",
    "name" : "reaper_reapack-arm64.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 6039797760,
    "size" : 4243456,
    "uuid" : "5ae5b818-2553-3924-89df-94afbfb7c922",
    "path" : "\/Library\/Application Support\/REAPER\/*\/reaper_sws-arm64.dylib",
    "name" : "reaper_sws-arm64.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 5259247616,
    "CFBundleIdentifier" : "com.meldaproduction.MStereoScope",
    "size" : 131072,
    "uuid" : "995af32c-dc8d-349e-ab7d-f47c18f2666d",
    "path" : "\/Library\/Audio\/Plug-Ins\/VST3\/MeldaProduction\/Stereo\/MStereoScope.vst3\/Contents\/MacOS\/MStereoScope.bin",
    "name" : "MStereoScope.bin",
    "CFBundleVersion" : "17.10"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 481421557760,
    "size" : 46088192,
    "uuid" : "2f2f4952-9d06-330c-9cc5-04ac8a6a1f5f",
    "path" : "\/Library\/Application Support\/MeldaProduction\/*\/libMeldaProductionAudioPluginKernelV11.dylib",
    "name" : "libMeldaProductionAudioPluginKernelV11.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 481312374784,
    "CFBundleShortVersionString" : "3.23",
    "CFBundleIdentifier" : "com.voxengo.audio-plugins.VST3.SPAN",
    "size" : 7634944,
    "uuid" : "3aa1dc06-f953-34a7-82d9-9fb75dc2b173",
    "path" : "\/Library\/Audio\/Plug-Ins\/VST3\/SPAN.vst3\/Contents\/MacOS\/SPAN",
    "name" : "SPAN",
    "CFBundleVersion" : "3.23"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 481320337408,
    "CFBundleShortVersionString" : "1.9",
    "CFBundleIdentifier" : "com.voxengo.audio-plugins.VST3.Correlometer",
    "size" : 6438912,
    "uuid" : "056e4443-5c3e-33a4-b988-b4e1aefafe48",
    "path" : "\/Library\/Audio\/Plug-Ins\/VST3\/Correlometer.vst3\/Contents\/MacOS\/Correlometer",
    "name" : "Correlometer",
    "CFBundleVersion" : "1.9"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 481479360512,
    "CFBundleShortVersionString" : "2.5.12",
    "CFBundleIdentifier" : "com.Youlean.vst3.Youlean-Loudness-Meter-2",
    "size" : 7766016,
    "uuid" : "1591aaa9-1e2b-36d7-b3e6-4436ae564f01",
    "path" : "\/Library\/Audio\/Plug-Ins\/VST3\/Youlean Loudness Meter 2.vst3\/Contents\/MacOS\/Youlean Loudness Meter 2",
    "name" : "Youlean Loudness Meter 2",
    "CFBundleVersion" : "2.5.12"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 481304887296,
    "CFBundleShortVersionString" : "1.3.0",
    "CFBundleIdentifier" : "com.DsgDnB.Scyllascope",
    "size" : 3964928,
    "uuid" : "5c5a7267-7020-3aad-8619-b88f2444540a",
    "path" : "\/Library\/Audio\/Plug-Ins\/VST3\/Scyllascope.vst3\/Contents\/MacOS\/Scyllascope",
    "name" : "Scyllascope",
    "CFBundleVersion" : "1.3.0"
  },
  {
    "source" : "P",
    "arch" : "arm64",
    "base" : 482466217984,
    "CFBundleShortVersionString" : "1.0.0",
    "CFBundleIdentifier" : "com.nih-plug.satcomp",
    "size" : 3997696,
    "uuid" : "077b5954-300c-32ec-95c5-4de9f1b60db5",
    "path" : "\/Library\/Audio\/Plug-Ins\/VST3\/satcomp.vst3\/Contents\/MacOS\/satcomp",
    "name" : "satcomp",
    "CFBundleVersion" : "1.0.0"
  },
  {
    "source" : "P",
    "arch" : "arm64e",
    "base" : 481267630080,
    "CFBundleShortVersionString" : "1.0",
    "CFBundleIdentifier" : "com.apple.AppleMetalOpenGLRenderer",
    "size" : 409600,
    "uuid" : "140d0cef-f342-380f-b7e7-cad89a979ec2",
    "path" : "\/System\/Library\/Extensions\/AppleMetalOpenGLRenderer.bundle\/Contents\/MacOS\/AppleMetalOpenGLRenderer",
    "name" : "AppleMetalOpenGLRenderer",
    "CFBundleVersion" : "1"
  },
  {
    "source" : "P",
    "arch" : "arm64e",
    "base" : 6546030592,
    "size" : 256428,
    "uuid" : "f63bf418-8f75-34a4-aaf1-8caf2b9b6a05",
    "path" : "\/usr\/lib\/system\/libsystem_kernel.dylib",
    "name" : "libsystem_kernel.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64e",
    "base" : 6546288640,
    "size" : 51484,
    "uuid" : "5a1828c3-cbc2-3b85-bd07-c8ed10021a41",
    "path" : "\/usr\/lib\/system\/libsystem_pthread.dylib",
    "name" : "libsystem_pthread.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64e",
    "base" : 6544773120,
    "size" : 535716,
    "uuid" : "fba7b23e-aa60-3a90-9aa4-a7a2e0ad63ee",
    "path" : "\/usr\/lib\/system\/libsystem_c.dylib",
    "name" : "libsystem_c.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64e",
    "base" : 6546575360,
    "CFBundleShortVersionString" : "6.9",
    "CFBundleIdentifier" : "com.apple.CoreFoundation",
    "size" : 4787872,
    "uuid" : "727e905e-f344-3823-8e13-e7a096388321",
    "path" : "\/System\/Library\/Frameworks\/CoreFoundation.framework\/Versions\/A\/CoreFoundation",
    "name" : "CoreFoundation",
    "CFBundleVersion" : "5027.0.69"
  },
  {
    "source" : "P",
    "arch" : "arm64e",
    "base" : 6570864640,
    "CFBundleShortVersionString" : "6.9",
    "CFBundleIdentifier" : "com.apple.Foundation",
    "size" : 19649160,
    "uuid" : "4317f7c6-73d2-3be9-b010-f318f1198a1b",
    "path" : "\/System\/Library\/Frameworks\/Foundation.framework\/Versions\/C\/Foundation",
    "name" : "Foundation",
    "CFBundleVersion" : "5027.0.69"
  },
  {
    "source" : "P",
    "arch" : "arm64e",
    "base" : 6619967488,
    "CFBundleShortVersionString" : "6.9",
    "CFBundleIdentifier" : "com.apple.AppKit",
    "size" : 23340300,
    "uuid" : "2688afbc-5c39-3132-bcf5-8d00d32d7615",
    "path" : "\/System\/Library\/Frameworks\/AppKit.framework\/Versions\/C\/AppKit",
    "name" : "AppKit",
    "CFBundleVersion" : "2775.10.103.1"
  },
  {
    "source" : "P",
    "arch" : "arm64e",
    "base" : 6768840704,
    "CFBundleShortVersionString" : "2.1.1",
    "CFBundleIdentifier" : "com.apple.HIToolbox",
    "size" : 3032320,
    "uuid" : "5d739a34-36b7-39b9-ba3b-7608b59c07a8",
    "path" : "\/System\/Library\/Frameworks\/Carbon.framework\/Versions\/A\/Frameworks\/HIToolbox.framework\/Versions\/A\/HIToolbox",
    "name" : "HIToolbox"
  },
  {
    "source" : "P",
    "arch" : "arm64e",
    "base" : 6542090240,
    "size" : 784100,
    "uuid" : "d341ddcb-7969-3a9c-8e03-0cf4751398a9",
    "path" : "\/usr\/lib\/dyld",
    "name" : "dyld"
  },
  {
    "size" : 0,
    "source" : "A",
    "base" : 0,
    "uuid" : "00000000-0000-0000-0000-000000000000"
  },
  {
    "source" : "P",
    "arch" : "arm64e",
    "base" : 7110008832,
    "CFBundleShortVersionString" : "2.0",
    "CFBundleIdentifier" : "com.apple.audio.midi.CoreMIDI",
    "size" : 811860,
    "uuid" : "da3e2cf1-d70c-3055-bf59-0f604da694a9",
    "path" : "\/System\/Library\/Frameworks\/CoreMIDI.framework\/Versions\/A\/CoreMIDI",
    "name" : "CoreMIDI",
    "CFBundleVersion" : "88"
  },
  {
    "source" : "P",
    "arch" : "arm64e",
    "base" : 6545309696,
    "size" : 610356,
    "uuid" : "1e9ba4aa-b5da-3cf3-80ac-4b880049573f",
    "path" : "\/usr\/lib\/libc++.1.dylib",
    "name" : "libc++.1.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64e",
    "base" : 6544470016,
    "size" : 287136,
    "uuid" : "022bf08f-0b3a-3225-9e65-16d1553ce90d",
    "path" : "\/usr\/lib\/system\/libdispatch.dylib",
    "name" : "libdispatch.dylib"
  },
  {
    "source" : "P",
    "arch" : "arm64e",
    "base" : 6763610112,
    "CFBundleShortVersionString" : "1.0",
    "CFBundleIdentifier" : "com.apple.audio.caulk",
    "size" : 172116,
    "uuid" : "c7ba2b3e-b175-30e9-abf6-4d568d9daa51",
    "path" : "\/System\/Library\/PrivateFrameworks\/caulk.framework\/Versions\/A\/caulk",
    "name" : "caulk"
  },
  {
    "source" : "P",
    "arch" : "arm64e",
    "base" : 6599626752,
    "CFBundleShortVersionString" : "5.0",
    "CFBundleIdentifier" : "com.apple.audio.CoreAudio",
    "size" : 7984692,
    "uuid" : "3ab8f076-7437-38aa-9f5b-b083cf020ada",
    "path" : "\/System\/Library\/Frameworks\/CoreAudio.framework\/Versions\/A\/CoreAudio",
    "name" : "CoreAudio",
    "CFBundleVersion" : "5.0"
  }
],
  "sharedCache" : {
  "base" : 6537347072,
  "size" : 7410368512,
  "uuid" : "c284a291-b68b-3d0d-a1a0-3f866f87efdc"
},
  "vmSummary" : "ReadOnly portion of Libraries: Total=3.1G resident=0K(0%) swapped_out_or_unallocated=3.1G(100%)\nWritable regions: Total=2.9G written=2547K(0%) resident=1731K(0%) swapped_out=816K(0%) unallocated=2.9G(100%)\n\n                                VIRTUAL   REGION \nREGION TYPE                        SIZE    COUNT (non-coalesced) \n===========                     =======  ======= \nAccelerate Framework               384K        3 \nActivity Tracing                   256K        1 \nAttributeGraph Data               1024K        1 \nCG Image                           224K       12 \nColorSync                           64K        4 \nCoreAnimation                     5088K      194 \nCoreGraphics                        64K        4 \nCoreImage                           32K        2 \nCoreServices                       176K        1 \nCoreUI Image Data                 3504K       31 \nDyld Private Memory                128K        1 \nFoundation                          48K        2 \nGuard                            385.0G        1 \nImage IO                           256K       16 \nMach Message                       400K        3 \nMalloc                             2.6G      164 \nMalloc Guard Page                  480K       30 \nMalloc Large (reserved)          213.2M        3         reserved VM address space (unallocated)\nOS Alloc Once                       32K        1 \nOpenGL GLSL                        256K        3 \nStack                             25.4M       31 \nStack Guard                       56.5M       31 \nUntagged                          2144K       43 \nUntagged (reserved)                768K        1         reserved VM address space (unallocated)\n__AUTH                            6690K      713 \n__AUTH_CONST                     100.0M     1067 \n__CTF                               824        1 \n__DATA                            99.0M     1165 \n__DATA_CONST                      42.2M     1138 \n__DATA_DIRTY                      14.7M      952 \n__FONT_DATA                        2352        1 \n__GLSLBUILTINS                    5176K        1 \n__LINKEDIT                         1.6G       70 \n__OBJC_RO                        118.0M        1 \n__OBJC_RW                         2735K        1 \n__TEXT                             1.5G     1227 \n__TPRO_CONST                       132K        4 \nmapped file                      528.5M       66 \npage table in kernel              1731K        1 \nshared memory                     1040K       25 \n===========                     =======  ======= \nTOTAL                            391.9G     7016 \nTOTAL, minus reserved VM space   391.7G     7016 \n",
  "legacyInfo" : {
  "threadTriggered" : {
    "name" : "reaper",
    "queue" : "com.apple.main-thread"
  }
},
  "logWritingSignature" : "0133e4cb1ce286f8efa45d79fecd17517f11b121",
  "roots_installed" : 0,
  "bug_type" : "309",
  "trmStatus" : 1,
  "trialInfo" : {
  "rollouts" : [
    {
      "rolloutId" : "6297d96be2c9387df974efa4",
      "factorPackIds" : [

      ],
      "deploymentId" : 240000032
    },
    {
      "rolloutId" : "67648e5334a82511f4acf879",
      "factorPackIds" : [

      ],
      "deploymentId" : 240000008
    }
  ],
  "experiments" : [

  ]
}
}

Model: Mac16,1, BootROM 20457.1.29, proc 10:0:4:6 processors, 16 GB, SMC 
Graphics: Apple M4, Apple M4, Built-In
Display: Color LCD, 3024 x 1964 Retina, Main, MirrorOff, Online
Display: LS24F32xG, 1920 x 1080 (1080p FHD - Full High Definition), MirrorOff, Online
Memory Module: LPDDR5, Micron
AirPort: spairport_wireless_card_type_wifi (0x14E4, 0x4388), wl0: Jul  2 2026 00:44:50 version 23.10.1039.0.41.51.239 FWID 01-974167da
IO80211_driverkit-1585.80 "IO80211_driverkit-1585.80" Sep 18 2026 22:02:52
AirPort: 
Bluetooth: Version (null), 0 services, 0 devices, 0 incoming serial ports
Network Service: Wi-Fi, AirPort, en0
Thunderbolt Bus: MacBook Pro, Apple Inc.
Thunderbolt Bus: MacBook Pro, Apple Inc.
Thunderbolt Bus: MacBook Pro, Apple Inc.
