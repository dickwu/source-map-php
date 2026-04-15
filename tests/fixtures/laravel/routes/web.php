<?php

use App\Http\Controllers\PatientConsentController;
use Illuminate\Support\Facades\Route;

Route::post('/patients/{patient}/consents', [PatientConsentController::class, 'store'])
    ->name('patients.consents.store')
    ->middleware('auth');
